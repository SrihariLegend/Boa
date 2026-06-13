#!/usr/bin/env python3
"""
SPRT testing tool for the Karpov chess engine.

Runs engine-vs-engine matches using cutechess-cli with SPRT stopping,
leveraging all 24 cores for maximum throughput.

Usage:
    # SPRT test: candidate vs baseline
    python3 tools/sprt_test.py --candidate ./target/release/karpov_new --baseline ./target/release/karpov

    # Gauntlet: measure Elo against Stockfish at various levels
    python3 tools/sprt_test.py --gauntlet

    # Quick non-regression test (just confirm no loss)
    python3 tools/sprt_test.py --regression --candidate ./target/release/karpov_new

    # Custom match against a specific Stockfish Elo
    python3 tools/sprt_test.py --baseline stockfish --sf-elo 2000 --rounds 200

Examples:
    python3 tools/sprt_test.py --gauntlet --rounds 100
    python3 tools/sprt_test.py --candidate ./karpov_v2 --baseline ./karpov_v1 --sprt 0 5
"""

import argparse
import os
import subprocess
import sys
import time
import re
from datetime import datetime

# ── Paths ────────────────────────────────────────────────────────────────────

DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOLS_DIR = os.path.dirname(os.path.abspath(__file__))
CUTECHESS = os.path.join(TOOLS_DIR, "cutechess-cli")
KARPOV_BIN = os.path.join(DIR, "target", "release", "karpov")
OPENINGS = os.path.join(TOOLS_DIR, "openings.epd")
PGN_DIR = os.path.join(TOOLS_DIR, "match_results")
CONCURRENCY = 24

# ── Colors ───────────────────────────────────────────────────────────────────

BOLD = "\033[1m"
DIM = "\033[2m"
GREEN = "\033[32m"
RED = "\033[31m"
YELLOW = "\033[33m"
CYAN = "\033[36m"
RESET = "\033[0m"


# ── Helpers ──────────────────────────────────────────────────────────────────

def ensure_dirs():
    os.makedirs(PGN_DIR, exist_ok=True)


def build_engine(label="karpov"):
    """Build the engine from source."""
    print(f"  {DIM}Building {label}...{RESET}", end=" ", flush=True)
    result = subprocess.run(
        f"cargo build --release -j {CONCURRENCY}",
        shell=True, capture_output=True, text=True, cwd=DIR, timeout=120,
    )
    if result.returncode != 0:
        print(f"{RED}FAILED{RESET}")
        print(result.stderr)
        return False
    print(f"{GREEN}OK{RESET}")
    return True


def timestamp():
    return datetime.now().strftime("%Y%m%d_%H%M%S")


def parse_cutechess_output(output):
    """Parse cutechess-cli output for results."""
    results = {
        "score": None,
        "elo": None,
        "elo_error": None,
        "los": None,
        "draw_ratio": None,
        "sprt_result": None,
        "games": 0,
        "wins": 0,
        "losses": 0,
        "draws": 0,
    }

    for line in output.split("\n"):
        # Score line: "Score of A vs B: 10 - 5 - 3  [0.639] 18"
        m = re.search(r"Score of .+ vs .+: (\d+) - (\d+) - (\d+)\s+\[([0-9.]+)\]\s+(\d+)", line)
        if m:
            results["wins"] = int(m.group(1))
            results["losses"] = int(m.group(2))
            results["draws"] = int(m.group(3))
            results["score"] = float(m.group(4))
            results["games"] = int(m.group(5))

        # Elo line: "Elo difference: 45 +/- 30, LOS: 98.5 %, DrawRatio: 25.0 %"
        m = re.search(r"Elo difference: ([+-]?\d+(?:\.\d+)?|inf|-inf) \+/- (\d+(?:\.\d+)?|nan)", line)
        if m:
            try:
                results["elo"] = float(m.group(1))
            except ValueError:
                results["elo"] = float("inf") if "inf" in m.group(1) else None
            try:
                results["elo_error"] = float(m.group(2))
            except ValueError:
                results["elo_error"] = None

        m = re.search(r"LOS: ([0-9.]+) %", line)
        if m:
            results["los"] = float(m.group(1))

        m = re.search(r"DrawRatio: ([0-9.]+) %", line)
        if m:
            results["draw_ratio"] = float(m.group(1))

        # SPRT result
        if "SPRT: llr" in line:
            if "H1 was accepted" in line:
                results["sprt_result"] = "PASSED"
            elif "H0 was accepted" in line:
                results["sprt_result"] = "FAILED"
            else:
                results["sprt_result"] = "INCONCLUSIVE"

    return results


def run_match(engine1_cmd, engine1_name, engine2_cmd, engine2_name,
              engine1_options=None, engine2_options=None,
              rounds=200, sprt=None, tc="10+0.1", pgn_file=None,
              draw_adjudicate=True, resign_adjudicate=True):
    """Run a cutechess-cli match and return parsed results."""

    cmd = [CUTECHESS]

    # Engine 1 — each option is a separate token
    e1_parts = [f"cmd={engine1_cmd}", "proto=uci", f"name={engine1_name}"]
    if engine1_options:
        for k, v in engine1_options.items():
            e1_parts.append(f"option.{k}={v}")
    cmd.extend(["-engine"] + e1_parts)

    # Engine 2 — each option is a separate token
    e2_parts = [f"cmd={engine2_cmd}", "proto=uci", f"name={engine2_name}"]
    if engine2_options:
        for k, v in engine2_options.items():
            e2_parts.append(f"option.{k}={v}")
    cmd.extend(["-engine"] + e2_parts)

    # Shared options
    cmd.extend(["-each", f"proto=uci", f"tc={tc}"])
    cmd.extend(["-rounds", str(rounds)])
    cmd.extend(["-concurrency", str(CONCURRENCY)])
    cmd.extend(["-openings", f"file={OPENINGS}", "format=epd", "order=random", "policy=round"])
    cmd.extend(["-repeat"])
    cmd.extend(["-recover"])

    # Adjudication
    if draw_adjudicate:
        cmd.extend(["-draw", "movenumber=40", "movecount=8", "score=10"])
    if resign_adjudicate:
        cmd.extend(["-resign", "movecount=5", "score=700", "twosided=true"])

    # Max game length
    cmd.extend(["-maxmoves", "200"])

    # SPRT
    if sprt:
        elo0, elo1 = sprt
        cmd.extend(["-sprt", f"elo0={elo0}", f"elo1={elo1}", "alpha=0.05", "beta=0.05"])

    # PGN output
    if pgn_file:
        cmd.extend(["-pgnout", pgn_file])

    # Rating interval
    cmd.extend(["-ratinginterval", "10"])

    print(f"\n  {DIM}Command: {' '.join(cmd)}{RESET}\n")

    t0 = time.time()
    process = subprocess.Popen(
        cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
    )

    output_lines = []
    for line in process.stdout:
        line = line.rstrip()
        output_lines.append(line)

        # Print progress lines
        if line.startswith("Score of") or "Elo difference" in line or "SPRT" in line:
            print(f"  {line}")
        elif line.startswith("Finished game"):
            # Print compact progress
            m = re.search(r"Finished game (\d+)", line)
            if m:
                game_num = int(m.group(1))
                if game_num % 10 == 0:
                    sys.stdout.write(f"\r  {DIM}Games completed: {game_num}{RESET}")
                    sys.stdout.flush()

    process.wait()
    elapsed = time.time() - t0

    full_output = "\n".join(output_lines)
    results = parse_cutechess_output(full_output)

    print(f"\n  {DIM}Completed in {elapsed:.0f}s ({results['games']} games){RESET}")

    return results, full_output


# ── Match Types ──────────────────────────────────────────────────────────────

def run_sprt_test(candidate, baseline, elo0=0, elo1=5, tc="10+0.1", max_rounds=2000):
    """Run SPRT test: is candidate better than baseline by [elo0, elo1]?"""
    ensure_dirs()
    ts = timestamp()
    pgn = os.path.join(PGN_DIR, f"sprt_{ts}.pgn")

    print(f"\n{BOLD}{'='*60}{RESET}")
    print(f"{BOLD}  SPRT Test{RESET}")
    print(f"  Candidate: {CYAN}{candidate}{RESET}")
    print(f"  Baseline:  {CYAN}{baseline}{RESET}")
    print(f"  H0: Elo ≤ {elo0}  H1: Elo ≥ {elo1}  α=0.05 β=0.05")
    print(f"  TC: {tc}  Max rounds: {max_rounds}  Concurrency: {CONCURRENCY}")
    print(f"  Openings: {OPENINGS}")
    print(f"  PGN: {pgn}")
    print(f"{BOLD}{'='*60}{RESET}")

    results, output = run_match(
        candidate, "Candidate",
        baseline, "Baseline",
        rounds=max_rounds,
        sprt=(elo0, elo1),
        tc=tc,
        pgn_file=pgn,
    )

    # Final verdict
    print(f"\n{BOLD}{'─'*60}{RESET}")
    print(f"  {BOLD}Results:{RESET}")
    print(f"    Games:  {results['games']}")
    print(f"    Score:  +{results['wins']} ={results['draws']} -{results['losses']}")

    if results["elo"] is not None:
        elo_str = f"{results['elo']:+.1f}" if results['elo'] != float('inf') else "+inf"
        err_str = f"±{results['elo_error']:.1f}" if results['elo_error'] else "±?"
        print(f"    Elo:    {elo_str} {err_str}")

    if results["los"] is not None:
        print(f"    LOS:    {results['los']:.1f}%")

    if results["sprt_result"] == "PASSED":
        print(f"\n  {GREEN}{BOLD}✓ SPRT PASSED — candidate is stronger!{RESET}")
    elif results["sprt_result"] == "FAILED":
        print(f"\n  {RED}{BOLD}✗ SPRT FAILED — candidate is NOT stronger.{RESET}")
    else:
        print(f"\n  {YELLOW}{BOLD}? SPRT INCONCLUSIVE — max rounds reached.{RESET}")

    print(f"{BOLD}{'─'*60}{RESET}\n")

    # Save summary
    summary_file = os.path.join(PGN_DIR, f"sprt_{ts}_summary.txt")
    with open(summary_file, "w") as f:
        f.write(f"SPRT Test: {ts}\n")
        f.write(f"Candidate: {candidate}\n")
        f.write(f"Baseline: {baseline}\n")
        f.write(f"H0: Elo ≤ {elo0}, H1: Elo ≥ {elo1}\n")
        f.write(f"TC: {tc}\n")
        f.write(f"Games: {results['games']}\n")
        f.write(f"Score: +{results['wins']} ={results['draws']} -{results['losses']}\n")
        if results['elo'] is not None:
            f.write(f"Elo: {results['elo']:+.1f} ±{results.get('elo_error', '?')}\n")
        f.write(f"SPRT: {results['sprt_result']}\n")

    return results


def run_regression_test(candidate, baseline=None, tc="10+0.1", max_rounds=1000):
    """Non-regression test: confirm candidate doesn't lose more than 5 Elo."""
    if baseline is None:
        baseline = KARPOV_BIN
    return run_sprt_test(candidate, baseline, elo0=-5, elo1=0, tc=tc, max_rounds=max_rounds)


def run_gauntlet(engine=None, elo_levels=None, rounds_per_level=100, tc="10+0.1"):
    """Run gauntlet against Stockfish at various Elo levels."""
    if engine is None:
        engine = KARPOV_BIN
    if elo_levels is None:
        elo_levels = [1320, 1500, 1700, 1900, 2100, 2300, 2500]

    ensure_dirs()
    ts = timestamp()

    print(f"\n{BOLD}{'='*60}{RESET}")
    print(f"{BOLD}  Elo Gauntlet — Karpov vs Stockfish{RESET}")
    print(f"  Engine:      {CYAN}{engine}{RESET}")
    print(f"  Elo levels:  {elo_levels}")
    print(f"  Rounds/lvl:  {rounds_per_level}")
    print(f"  TC:          {tc}")
    print(f"  Concurrency: {CONCURRENCY}")
    print(f"{BOLD}{'='*60}{RESET}")

    all_results = []

    for sf_elo in elo_levels:
        pgn = os.path.join(PGN_DIR, f"gauntlet_{ts}_vs{sf_elo}.pgn")

        print(f"\n{BOLD}  ── Stockfish @ {sf_elo} Elo ──{RESET}")

        results, output = run_match(
            engine, "Karpov",
            "stockfish", f"SF_{sf_elo}",
            engine2_options={
                "UCI_LimitStrength": "true",
                "UCI_Elo": str(sf_elo),
                "Threads": "1",
                "Hash": "16",
            },
            rounds=rounds_per_level,
            tc=tc,
            pgn_file=pgn,
        )

        all_results.append((sf_elo, results))

        # Quick verdict
        w, d, l = results["wins"], results["draws"], results["losses"]
        total = w + d + l
        score_pct = (w + d * 0.5) / total * 100 if total > 0 else 0

        if score_pct >= 60:
            verdict = f"{GREEN}DOMINANT{RESET}"
        elif score_pct >= 45:
            verdict = f"{YELLOW}COMPETITIVE{RESET}"
        else:
            verdict = f"{RED}OUTMATCHED{RESET}"

        elo_str = f"{results['elo']:+.0f}±{results['elo_error']:.0f}" if results['elo'] and results['elo_error'] else "N/A"
        print(f"  Result: +{w} ={d} -{l} ({score_pct:.0f}%) Elo: {elo_str}  {verdict}")

    # Summary table
    print(f"\n{BOLD}{'='*60}{RESET}")
    print(f"{BOLD}  GAUNTLET SUMMARY{RESET}")
    print(f"  {'SF Elo':>7} │ {'Score':>12} │ {'Win%':>5} │ {'Elo Diff':>10} │ Verdict")
    print(f"  {'─'*7}─┼─{'─'*12}─┼─{'─'*5}─┼─{'─'*10}─┼─{'─'*12}")

    estimated_elo = None
    for sf_elo, r in all_results:
        w, d, l = r["wins"], r["draws"], r["losses"]
        total = w + d + l
        score_pct = (w + d * 0.5) / total * 100 if total > 0 else 0
        elo_str = f"{r['elo']:+.0f}±{r['elo_error']:.0f}" if r['elo'] and r['elo_error'] else "N/A"

        if score_pct >= 60:
            verdict = f"{GREEN}>>>{RESET}"
        elif score_pct >= 45:
            verdict = f"{YELLOW}≈{RESET}"
        else:
            verdict = f"{RED}<<<{RESET}"

        print(f"  {sf_elo:>7} │ +{w:>3} ={d:>3} -{l:>3} │ {score_pct:>4.0f}% │ {elo_str:>10} │ {verdict}")

        # Estimate Karpov's Elo: find where score crosses 50%
        if r['elo'] is not None and r['elo'] != float('inf'):
            estimated_elo = sf_elo + r['elo']

    if estimated_elo is not None:
        print(f"\n  {BOLD}Estimated Karpov Elo: ~{estimated_elo:.0f}{RESET}")
        print(f"  {DIM}(Based on last level tested — run more levels for accuracy){RESET}")

    print(f"{BOLD}{'='*60}{RESET}\n")

    # Save summary
    summary_file = os.path.join(PGN_DIR, f"gauntlet_{ts}_summary.txt")
    with open(summary_file, "w") as f:
        f.write(f"Gauntlet: {ts}\n")
        f.write(f"Engine: {engine}\n")
        f.write(f"TC: {tc}\n\n")
        for sf_elo, r in all_results:
            w, d, l = r["wins"], r["draws"], r["losses"]
            total = w + d + l
            score_pct = (w + d * 0.5) / total * 100 if total > 0 else 0
            elo_str = f"{r['elo']:+.0f}±{r['elo_error']:.0f}" if r['elo'] and r['elo_error'] else "N/A"
            f.write(f"vs SF_{sf_elo}: +{w} ={d} -{l} ({score_pct:.0f}%) Elo: {elo_str}\n")
        if estimated_elo:
            f.write(f"\nEstimated Elo: ~{estimated_elo:.0f}\n")

    return all_results


def run_single_match(engine, opponent, opponent_name, opponent_options=None,
                     rounds=200, tc="10+0.1"):
    """Run a single match against any engine."""
    ensure_dirs()
    ts = timestamp()
    pgn = os.path.join(PGN_DIR, f"match_{ts}.pgn")

    print(f"\n{BOLD}{'='*60}{RESET}")
    print(f"{BOLD}  Match: Karpov vs {opponent_name}{RESET}")
    print(f"  Rounds: {rounds}  TC: {tc}  Concurrency: {CONCURRENCY}")
    print(f"{BOLD}{'='*60}{RESET}")

    results, output = run_match(
        engine, "Karpov",
        opponent, opponent_name,
        engine2_options=opponent_options,
        rounds=rounds,
        tc=tc,
        pgn_file=pgn,
    )

    w, d, l = results["wins"], results["draws"], results["losses"]
    total = w + d + l
    score_pct = (w + d * 0.5) / total * 100 if total > 0 else 0

    print(f"\n  {BOLD}Final: +{w} ={d} -{l} ({score_pct:.0f}%){RESET}")
    if results["elo"] is not None:
        elo_str = f"{results['elo']:+.0f}" if results['elo'] != float('inf') else "+inf"
        err_str = f"±{results['elo_error']:.0f}" if results['elo_error'] else ""
        print(f"  Elo: {elo_str}{err_str}")

    return results


# ── Main ─────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="SPRT testing tool for Karpov chess engine",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Run Elo gauntlet (recommended first step)
  python3 tools/sprt_test.py --gauntlet

  # Quick gauntlet with fewer games
  python3 tools/sprt_test.py --gauntlet --rounds 50

  # SPRT test: new build vs old build
  python3 tools/sprt_test.py --candidate ./karpov_new --baseline ./karpov_old

  # Non-regression test
  python3 tools/sprt_test.py --regression --candidate ./karpov_new

  # Match against SF at specific Elo
  python3 tools/sprt_test.py --match --sf-elo 2000 --rounds 200
        """,
    )

    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--gauntlet", action="store_true",
                      help="Run Elo gauntlet against Stockfish at various levels")
    mode.add_argument("--sprt", nargs=2, type=float, metavar=("ELO0", "ELO1"),
                      help="Run SPRT test with bounds [ELO0, ELO1]")
    mode.add_argument("--regression", action="store_true",
                      help="Non-regression test (confirm no Elo loss)")
    mode.add_argument("--match", action="store_true",
                      help="Run a match against Stockfish at a specific Elo")

    parser.add_argument("--candidate", type=str, default=None,
                        help="Path to candidate engine binary")
    parser.add_argument("--baseline", type=str, default=None,
                        help="Path to baseline engine binary (default: current build)")
    parser.add_argument("--rounds", type=int, default=200,
                        help="Number of game rounds (default: 200)")
    parser.add_argument("--tc", type=str, default="10+0.1",
                        help="Time control (default: 10+0.1)")
    parser.add_argument("--sf-elo", type=int, default=2000,
                        help="Stockfish Elo for --match mode (default: 2000)")
    parser.add_argument("--elo-levels", type=str, default=None,
                        help="Comma-separated Elo levels for gauntlet (default: 1320,1500,1700,1900,2100,2300,2500)")
    parser.add_argument("--build", action="store_true",
                        help="Build the engine from source before testing")

    args = parser.parse_args()

    # Validate
    if not os.path.exists(CUTECHESS):
        print(f"{RED}Error: cutechess-cli not found at {CUTECHESS}{RESET}")
        print(f"Build it first or update the CUTECHESS path in this script.")
        sys.exit(1)

    if not os.path.exists(OPENINGS):
        print(f"{RED}Error: openings file not found at {OPENINGS}{RESET}")
        sys.exit(1)

    # Build if requested
    if args.build:
        if not build_engine():
            sys.exit(1)

    # Resolve engine paths
    baseline = args.baseline or KARPOV_BIN
    candidate = args.candidate or KARPOV_BIN

    for path in [candidate, baseline]:
        if path != "stockfish" and not os.path.exists(path):
            print(f"{RED}Error: engine binary not found at {path}{RESET}")
            sys.exit(1)

    # Run the selected mode
    if args.gauntlet:
        levels = None
        if args.elo_levels:
            levels = [int(x) for x in args.elo_levels.split(",")]
        run_gauntlet(engine=candidate, elo_levels=levels,
                     rounds_per_level=args.rounds, tc=args.tc)

    elif args.sprt:
        elo0, elo1 = args.sprt
        run_sprt_test(candidate, baseline, elo0=elo0, elo1=elo1,
                      tc=args.tc, max_rounds=args.rounds)

    elif args.regression:
        run_regression_test(candidate, baseline, tc=args.tc, max_rounds=args.rounds)

    elif args.match:
        run_single_match(
            candidate, "stockfish", f"SF_{args.sf_elo}",
            opponent_options={
                "UCI_LimitStrength": "true",
                "UCI_Elo": str(args.sf_elo),
                "Threads": "1",
                "Hash": "16",
            },
            rounds=args.rounds, tc=args.tc,
        )


if __name__ == "__main__":
    main()
