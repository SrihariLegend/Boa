#!/usr/bin/env python3
"""Tune correction history pruning weights via node-count minimisation.

Runs `boa bench` (20 positions, depth 10) with different correction weight
combinations.  Fewer total nodes = more accurate pruning = better weights.

Weights are passed via env vars BOA_CORR_W_RFP, BOA_CORR_W_NMP, BOA_CORR_W_FFP.

Usage:
  python3 tools/tune_correction.py sweep    # independent parameter sweeps
  python3 tools/tune_correction.py grid     # 3x3x3 grid near best values
  python3 tools/tune_correction.py all      # sweep + grid + baseline
"""

import subprocess
import os
import sys

BINARY = "./target/release/boa"
POSITIONS = 20
DEPTH = 10

# Default (current) weights
DEFAULT_RFP = 2
DEFAULT_NMP = 1
DEFAULT_FFP = 1


def bench(rfp: int, nmp: int, ffp: int, runs: int = 3) -> float:
    """Run bench and return median total nodes (lower is better)."""
    env = os.environ.copy()
    env["BOA_CORR_W_RFP"] = str(rfp)
    env["BOA_CORR_W_NMP"] = str(nmp)
    env["BOA_CORR_W_FFP"] = str(ffp)

    nodes = []
    for _ in range(runs):
        try:
            out = subprocess.check_output(
                [BINARY],
                input=b"bench\n",
                env=env,
                stderr=subprocess.DEVNULL,
                timeout=120,
            )
            # Last line is total nodes
            total = int(out.decode().strip().split("\n")[-1])
            nodes.append(total)
        except (subprocess.TimeoutExpired, ValueError, IndexError) as e:
            print(f"  FAILED (rfp={rfp}, nmp={nmp}, ffp={ffp}): {e}", file=sys.stderr)
            return float("inf")

    nodes.sort()
    return nodes[len(nodes) // 2]  # median


def fmt(n: float) -> str:
    if n == float("inf"):
        return "FAILED"
    return f"{n/1_000_000:.2f}M"


def sweep():
    """Independent 1D sweeps — vary one weight at a time."""
    print("=" * 65)
    print("SWEEP: vary one weight at a time, others at default")
    print("=" * 65)

    # Baseline
    base = bench(DEFAULT_RFP, DEFAULT_NMP, DEFAULT_FFP)
    print(f"\n  baseline (RFP={DEFAULT_RFP} NMP={DEFAULT_NMP} FFP={DEFAULT_FFP}): {fmt(base)}")

    # RFP sweep
    print(f"\n  {'RFP':>5}  {'nodes':>10}")
    print("  " + "-" * 20)
    for rfp in range(0, 9):
        n = bench(rfp, DEFAULT_NMP, DEFAULT_FFP, runs=2)
        marker = " <--" if rfp == DEFAULT_RFP else ""
        print(f"  {rfp:>5}  {fmt(n):>10}{marker}")

    # NMP sweep
    print(f"\n  {'NMP':>5}  {'nodes':>10}")
    print("  " + "-" * 20)
    for nmp in range(0, 7):
        n = bench(DEFAULT_RFP, nmp, DEFAULT_FFP, runs=2)
        marker = " <--" if nmp == DEFAULT_NMP else ""
        print(f"  {nmp:>5}  {fmt(n):>10}{marker}")

    # FFP sweep
    print(f"\n  {'FFP':>5}  {'nodes':>10}")
    print("  " + "-" * 20)
    for ffp in range(0, 7):
        n = bench(DEFAULT_RFP, DEFAULT_NMP, ffp, runs=2)
        marker = " <--" if ffp == DEFAULT_FFP else ""
        print(f"  {ffp:>5}  {fmt(n):>10}{marker}")


def grid():
    """3x3x3 grid search around the best values from sweep."""
    print("\n" + "=" * 65)
    print("GRID: 3x3x3 search")
    print("=" * 65)

    # Centered on current defaults — adjust after sweep results
    rfp_vals = [DEFAULT_RFP - 1, DEFAULT_RFP, DEFAULT_RFP + 1]
    nmp_vals = [DEFAULT_NMP - 1, DEFAULT_NMP, DEFAULT_NMP + 1]
    ffp_vals = [DEFAULT_FFP - 1, DEFAULT_FFP, DEFAULT_FFP + 1]

    results = []
    for rfp in rfp_vals:
        for nmp in nmp_vals:
            for ffp in ffp_vals:
                if rfp < 0 or nmp < 0 or ffp < 0:
                    continue
                n = bench(rfp, nmp, ffp, runs=2)
                results.append((n, rfp, nmp, ffp))
                print(f"  RFP={rfp} NMP={nmp} FFP={ffp}  →  {fmt(n)}")

    results.sort()
    print(f"\n  Best: RFP={results[0][1]} NMP={results[0][2]} FFP={results[0][3]}  →  {fmt(results[0][0])}")
    return results[0]


def main():
    if len(sys.argv) < 2:
        print("Usage: python3 tools/tune_correction.py [sweep|grid|all]")
        sys.exit(1)

    cmd = sys.argv[1]

    if not os.path.exists(BINARY):
        print(f"Binary not found: {BINARY}. Build with: cargo build --release")
        sys.exit(1)

    if cmd in ("sweep", "all"):
        sweep()

    if cmd in ("grid", "all"):
        best = grid()
        print(f"\nFinal recommendation: BOA_CORR_W_RFP={best[1]} BOA_CORR_W_NMP={best[2]} BOA_CORR_W_FFP={best[3]}")


if __name__ == "__main__":
    main()
