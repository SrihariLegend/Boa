#!/usr/bin/env python3
"""
Self-play ablation test for Karpov chess engine.

For each eval term, builds a variant with that term disabled,
plays it against the baseline, and reports W/D/L + Elo diff.

Usage:
    python3 tools/selfplay_ablation.py [--games N] [--time T] [--terms TERM1,TERM2,...]

Example:
    python3 tools/selfplay_ablation.py
    python3 tools/selfplay_ablation.py --games 10 --time 0.05
    python3 tools/selfplay_ablation.py --terms mobility,king_safety
"""

import logging
logging.getLogger("chess.engine").setLevel(logging.CRITICAL)

import chess, chess.engine, asyncio, os, re, shutil, math, sys, subprocess, warnings, argparse, time
warnings.filterwarnings("ignore")

DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVAL_RS = os.path.join(DIR, "src", "eval.rs")
BIN = os.path.join(DIR, "target", "release", "karpov")

ALL_TERMS = [
    ("freedom_metric",  "freedom_metric",          "0"),
    ("piece_coord",     "piece_coordination_eval",  "(0, 0)"),
    ("weak_squares",    "weak_square_eval",         "(0, 0)"),
    ("king_safety",     "king_safety",              "(0, 0)"),
    ("mobility",        "mobility_and_activity",    "(0, 0)"),
    ("pawn_struct",     "pawn_structure",           "(0, 0)"),
    ("trade_down",      "trade_down_bonus",         "(0, 0)"),
    # ── New data-driven terms (combined into piece_coord or standalone) ──
    ("adv_pawns",       "advanced_pawn_eval",       "(0, 0)"),
    ("min_mobility",    "min_piece_mobility_eval",  "(0, 0)"),
]

OPENINGS = [
    "d2d4 d7d5",
    "e2e4 e7e5",
    "d2d4 g8f6 c2c4 e7e6",
    "e2e4 c7c5",
    "d2d4 d7d5 c2c4 e7e6",
    "g1f3 d7d5 g2g3",
    "e2e4 e7e6",
    "d2d4 g8f6 c2c4 g7g6",
    "e2e4 c7c6",
    "d2d4 d7d5 g1f3 g8f6",
]

MAX_MOVES = 150

# ── Colors ───────────────────────────────────────────────────
GREEN  = "\033[92m"
RED    = "\033[91m"
YELLOW = "\033[93m"
CYAN   = "\033[96m"
BOLD   = "\033[1m"
DIM    = "\033[2m"
RESET  = "\033[0m"


def build(code, name):
    with open(EVAL_RS, "w") as f:
        f.write(code)
    r = subprocess.run(
        "cargo build --release -j 24 2>&1",
        shell=True, capture_output=True, cwd=DIR, timeout=60
    )
    if r.returncode != 0:
        print(f"{RED}Build failed for {name}!{RESET}")
        print(r.stdout.decode()[-500:] if r.stdout else "")
        return None
    dst = os.path.join(DIR, "target", "release", name)
    # Kill any old process using this binary, then copy
    try:
        os.remove(dst)
    except FileNotFoundError:
        pass
    shutil.copy2(BIN, dst)
    return dst


def disable(code, func, zv):
    m = re.search(rf"(fn {func}\([^)]*\)[^{{]*\{{)", code)
    if not m:
        return None
    return code[: m.end()] + f"\n    return {zv};\n" + code[m.end() :]


async def play_game(white_bin, black_bin, opening, time_limit):
    tw, ew = await chess.engine.popen_uci(white_bin, stderr=asyncio.subprocess.DEVNULL)
    tb, eb = await chess.engine.popen_uci(black_bin, stderr=asyncio.subprocess.DEVNULL)
    board = chess.Board()
    for uci in opening.split():
        mv = chess.Move.from_uci(uci)
        if mv in board.legal_moves:
            board.push(mv)
    moves = 0
    try:
        while not board.is_game_over() and moves < MAX_MOVES:
            engine = ew if board.turn == chess.WHITE else eb
            result = await asyncio.wait_for(
                engine.play(board, chess.engine.Limit(time=time_limit)),
                timeout=10.0,
            )
            board.push(result.move)
            moves += 1
    except Exception:
        pass
    finally:
        for e in [ew, eb]:
            try:
                await e.quit()
            except Exception:
                pass
    if board.is_game_over():
        r = board.result()
        return ("W" if r == "1-0" else "L" if r == "0-1" else "D"), moves
    return "D", moves


def elo_diff(wins, losses, draws):
    total = wins + losses + draws
    if total == 0:
        return 0, 999
    score = (wins + draws * 0.5) / total
    if score <= 0.01:
        return -400, 999
    if score >= 0.99:
        return 400, 999
    e = -400 * math.log10(1 / score - 1)
    se = math.sqrt(score * (1 - score) / total)
    err = 400 * se / (score * (1 - score) * math.log(10))
    return e, err


def result_char(outcome, baseline_is_white):
    """Colored character showing game result from baseline's perspective."""
    if outcome == "D":
        return f"{DIM}={RESET}"
    baseline_won = (outcome == "W" and baseline_is_white) or (
        outcome == "L" and not baseline_is_white
    )
    if baseline_won:
        return f"{GREEN}B{RESET}"
    return f"{RED}V{RESET}"


def verdict_str(elo_val, err):
    if elo_val > err:
        return f"{GREEN}{BOLD}HELPS ✓{RESET}"
    if elo_val < -err:
        return f"{RED}{BOLD}HURTS ✗{RESET}"
    return f"{YELLOW}UNCLEAR{RESET}"


def print_header(time_limit, games_per_term):
    print()
    print(f"{BOLD}{'═' * 70}{RESET}")
    print(f"{BOLD}{CYAN}  Karpov Self-Play Ablation Test{RESET}")
    print(f"{BOLD}{'═' * 70}{RESET}")
    print(f"  {DIM}Time/move:{RESET} {time_limit*1000:.0f}ms  "
          f"{DIM}Games/term:{RESET} {games_per_term}  "
          f"{DIM}Max moves:{RESET} {MAX_MOVES}")
    print(f"  {DIM}Method:{RESET} Baseline (all terms) vs Variant (one term disabled)")
    print(f"  {DIM}B{RESET}=baseline win  {DIM}V{RESET}=variant win  {DIM}={RESET}=draw")
    print(f"{BOLD}{'─' * 70}{RESET}")
    print()


def print_summary(results):
    print()
    print(f"{BOLD}{'═' * 70}{RESET}")
    print(f"{BOLD}{CYAN}  SUMMARY{RESET}")
    print(f"{BOLD}{'═' * 70}{RESET}")
    print(f"  {BOLD}{'Term':<18} {'W':>3} {'D':>3} {'L':>3}  {'Elo':>7}  Verdict{RESET}")
    print(f"  {'─' * 55}")

    # Sort: most helpful first
    sorted_results = sorted(results, key=lambda r: -r[4])

    for name, w, l, d, e, err in sorted_results:
        elo_str = f"{e:+.0f}±{err:.0f}"
        v = verdict_str(e, err)
        print(f"  {name:<18} {w:>3} {d:>3} {l:>3}  {elo_str:>10}  {v}")

    print(f"  {'─' * 55}")
    print(f"  {DIM}Positive Elo = term helps (baseline beats variant){RESET}")
    print(f"  {DIM}Negative Elo = term hurts (variant beats baseline){RESET}")
    print()


async def run_ablation(terms, time_limit, num_openings):
    with open(EVAL_RS) as f:
        orig = f.read()
    bak = EVAL_RS + ".bak"
    shutil.copy2(EVAL_RS, bak)

    openings = OPENINGS[:num_openings]
    games_per_term = len(openings) * 2  # each opening played from both sides

    try:
        base = build(orig, "karpov_baseline")
        if not base:
            return

        print_header(time_limit, games_per_term)

        results = []
        for idx, (name, func, zv) in enumerate(terms):
            mod = disable(orig, func, zv)
            if not mod:
                print(f"  {RED}⚠ Could not find function '{func}' — skipping{RESET}")
                results.append((name, 0, 0, 0, 0, 999))
                continue

            sys.stdout.write(f"  {BOLD}[{idx+1}/{len(terms)}]{RESET} {name:<18} building...")
            sys.stdout.flush()

            var = build(mod, "karpov_variant")
            if not var:
                results.append((name, 0, 0, 0, 0, 999))
                continue

            # Clear "building..." text
            sys.stdout.write(f"\r  {BOLD}[{idx+1}/{len(terms)}]{RESET} {name:<18} ")
            sys.stdout.flush()

            wins, losses, draws = 0, 0, 0
            t0 = time.time()

            for i, op in enumerate(openings):
                for swap in [False, True]:
                    if swap:
                        w, b = var, base  # variant is white
                    else:
                        w, b = base, var  # baseline is white

                    outcome, nmoves = await play_game(w, b, op, time_limit)

                    # outcome is from white's perspective
                    baseline_is_white = not swap
                    if outcome == "W":
                        if baseline_is_white:
                            wins += 1
                        else:
                            losses += 1
                    elif outcome == "L":
                        if baseline_is_white:
                            losses += 1
                        else:
                            wins += 1
                    else:
                        draws += 1

                    sys.stdout.write(result_char(outcome, baseline_is_white))
                    sys.stdout.flush()

            elapsed = time.time() - t0
            e, err = elo_diff(wins, losses, draws)
            v = verdict_str(e, err)
            elo_str = f"{e:+.0f}±{err:.0f}"
            print(f"  {wins}W {draws}D {losses}L  {elo_str:>10}  {v}  {DIM}({elapsed:.0f}s){RESET}")
            results.append((name, wins, losses, draws, e, err))

        print_summary(results)

    finally:
        # Restore original eval.rs
        shutil.copy2(bak, EVAL_RS)
        os.remove(bak)
        subprocess.run(
            "cargo build --release -j 24 2>&1",
            shell=True, capture_output=True, cwd=DIR, timeout=60,
        )
        # Cleanup binaries
        for f in ["karpov_baseline", "karpov_variant"]:
            p = os.path.join(DIR, "target", "release", f)
            if os.path.exists(p):
                os.remove(p)


def main():
    parser = argparse.ArgumentParser(description="Karpov self-play ablation test")
    parser.add_argument("--games", type=int, default=20, help="Games per term (default 20, must be even)")
    parser.add_argument("--time", type=float, default=0.1, help="Seconds per move (default 0.1)")
    parser.add_argument("--terms", type=str, default="", help="Comma-separated term names to test (default: all)")
    args = parser.parse_args()

    num_openings = max(1, args.games // 2)
    if num_openings > len(OPENINGS):
        num_openings = len(OPENINGS)

    if args.terms:
        requested = [t.strip() for t in args.terms.split(",")]
        terms = [t for t in ALL_TERMS if t[0] in requested]
        missing = [r for r in requested if r not in [t[0] for t in terms]]
        if missing:
            print(f"{RED}Unknown terms: {', '.join(missing)}{RESET}")
            print(f"Available: {', '.join(t[0] for t in ALL_TERMS)}")
            return
    else:
        terms = ALL_TERMS

    asyncio.run(run_ablation(terms, args.time, num_openings))


if __name__ == "__main__":
    main()
