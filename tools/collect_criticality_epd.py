#!/usr/bin/env python3
"""Collect bounded criticality probes from an EPD suite without match managers.

The collector drives one Boa UCI process through EPD starting positions and then
lets Boa play a bounded number of plies from each position. Probe rows are
written by the engine's existing criticality logger. This script deliberately
prints only small aggregate progress/status lines and never reads probe shards.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import time
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", default="target/release/boa")
    parser.add_argument("--openings", default="tools/openings.epd")
    parser.add_argument("--out-dir", required=True)
    parser.add_argument("--depth", type=int, default=10)
    parser.add_argument("--plies", type=int, default=16, help="Self-play plies from each EPD position")
    parser.add_argument("--passes", type=int, default=1, help="Repeat the suite; useful only with randomized engine settings")
    parser.add_argument("--max-rows", type=int, default=500_000)
    parser.add_argument("--max-csv-mib", type=int, default=64)
    parser.add_argument("--max-total-mib", type=int, default=128)
    parser.add_argument("--lmr-probe-permille", type=int, default=10)
    parser.add_argument("--futility-probe-permille", type=int, default=1)
    parser.add_argument("--futility-borderline-probe-permille", type=int, default=100)
    parser.add_argument("--futility-borderline-threshold-cp", type=int, default=30)
    parser.add_argument("--futility-quiet-probe-permille", type=int, default=0)
    parser.add_argument("--futility-quiet-borderline-probe-permille", type=int, default=0)
    parser.add_argument("--futility-quiet-borderline-low-cp", type=int, default=-50)
    parser.add_argument("--futility-quiet-borderline-high-cp", type=int, default=100)
    parser.add_argument("--rfp-probe-permille", type=int, default=10)
    parser.add_argument("--rfp-borderline-probe-permille", type=int)
    parser.add_argument("--rfp-borderline-threshold-cp", type=int, default=40)
    args = parser.parse_args()

    if args.depth < 1 or args.plies < 1 or args.passes < 1:
        raise SystemExit("--depth, --plies, and --passes must be positive")

    openings = load_epd_fens(Path(args.openings))
    if not openings:
        raise SystemExit(f"no EPD FENs found in {args.openings}")

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    rfp_borderline_probe_permille = (
        args.rfp_borderline_probe_permille
        if args.rfp_borderline_probe_permille is not None
        else args.rfp_probe_permille
    )

    env = os.environ.copy()
    env.update(
        {
            "BOA_CRITICALITY_LOG_DIR": str(out_dir),
            "BOA_LMR_PROBE_PERMILLE": str(args.lmr_probe_permille),
            "BOA_FUTILITY_PROBE_PERMILLE": str(args.futility_probe_permille),
            "BOA_FUTILITY_BORDERLINE_PROBE_PERMILLE": str(args.futility_borderline_probe_permille),
            "BOA_FUTILITY_BORDERLINE_THRESHOLD_CP": str(args.futility_borderline_threshold_cp),
            "BOA_FUTILITY_QUIET_PROBE_PERMILLE": str(args.futility_quiet_probe_permille),
            "BOA_FUTILITY_QUIET_BORDERLINE_PROBE_PERMILLE": str(args.futility_quiet_borderline_probe_permille),
            "BOA_FUTILITY_QUIET_BORDERLINE_LOW_CP": str(args.futility_quiet_borderline_low_cp),
            "BOA_FUTILITY_QUIET_BORDERLINE_HIGH_CP": str(args.futility_quiet_borderline_high_cp),
            "BOA_RFP_PROBE_PERMILLE": str(args.rfp_probe_permille),
            "BOA_RFP_BORDERLINE_PROBE_PERMILLE": str(rfp_borderline_probe_permille),
            "BOA_RFP_BORDERLINE_THRESHOLD_CP": str(args.rfp_borderline_threshold_cp),
            "BOA_CRITICALITY_MAX_ROWS": str(args.max_rows),
            "BOA_CRITICALITY_MAX_CSV_BYTES": str(args.max_csv_mib * 1024 * 1024),
            "BOA_CRITICALITY_MAX_TOTAL_BYTES": str(args.max_total_mib * 1024 * 1024),
            "BOA_CRITICALITY_COMPRESS": "1",
        }
    )

    started = time.time()
    proc = subprocess.Popen(
        [str(Path(args.engine))],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        encoding="utf8",
        errors="replace",
        env=env,
        bufsize=1,
    )
    assert proc.stdin is not None and proc.stdout is not None

    try:
        send(proc, "uci")
        read_until(proc, "uciok")
        send(proc, "isready")
        read_until(proc, "readyok")

        searches = 0
        legal_moves = 0
        for pass_index in range(args.passes):
            for fen_index, fen in enumerate(openings):
                send(proc, "ucinewgame")
                moves: list[str] = []
                for _ply in range(args.plies):
                    position = f"position fen {fen}"
                    if moves:
                        position += " moves " + " ".join(moves)
                    send(proc, position)
                    send(proc, f"go depth {args.depth}")
                    bestmove = read_bestmove(proc)
                    searches += 1
                    if bestmove in ("", "0000", "(none)"):
                        break
                    moves.append(bestmove)
                    legal_moves += 1
                if (fen_index + 1) % 8 == 0:
                    print(
                        f"progress pass={pass_index + 1}/{args.passes} "
                        f"fen={fen_index + 1}/{len(openings)} searches={searches}",
                        flush=True,
                    )

        send(proc, "quit")
        proc.wait(timeout=10)
    finally:
        if proc.poll() is None:
            proc.kill()

    elapsed = max(0.001, time.time() - started)
    print(
        f"done openings={len(openings)} passes={args.passes} searches={searches} "
        f"moves={legal_moves} seconds={elapsed:.1f} out_dir={out_dir}",
        flush=True,
    )


def load_epd_fens(path: Path) -> list[str]:
    fens: list[str] = []
    with path.open("r", encoding="utf8", errors="replace") as handle:
        for raw in handle:
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            if len(parts) < 4:
                continue
            fens.append(" ".join(parts[:4] + ["0", "1"]))
    return fens


def send(proc: subprocess.Popen[str], command: str) -> None:
    assert proc.stdin is not None
    proc.stdin.write(command + "\n")
    proc.stdin.flush()


def read_until(proc: subprocess.Popen[str], marker: str) -> None:
    assert proc.stdout is not None
    while True:
        line = proc.stdout.readline()
        if line == "":
            raise RuntimeError(f"engine exited before {marker}")
        if line.strip() == marker:
            return


def read_bestmove(proc: subprocess.Popen[str]) -> str:
    assert proc.stdout is not None
    while True:
        line = proc.stdout.readline()
        if line == "":
            raise RuntimeError("engine exited before bestmove")
        parts = line.strip().split()
        if len(parts) >= 2 and parts[0] == "bestmove":
            return parts[1]


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(130)
