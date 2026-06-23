#!/usr/bin/env python3
"""
Compare tuned CG-FFP against an old plain-futility baseline on STS EPD files.

This script intentionally does NOT use cutechess or any match manager. It builds
two engine binaries in a temporary directory, runs fixed-depth UCI searches over
STS/*.epd, scores moves from STS c0 annotations, and writes compact CSV/text
results.

Default comparison:
  candidate = current working tree
  baseline  = git ref HEAD

If you commit the CG-FFP changes before running this, pass --baseline-ref with
the old plain-futility commit.
"""

from __future__ import annotations

import argparse
import csv
import datetime as dt
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

try:
    import chess  # type: ignore
except ImportError:
    print(
        "error: python package 'chess' is required. Install with: python3 -m pip install chess",
        file=sys.stderr,
    )
    sys.exit(2)


REPO = Path(__file__).resolve().parents[1]


@dataclass
class Position:
    suite: str
    line_no: int
    fen: str
    ident: str
    max_score: int
    bm: set[chess.Move]
    c0: dict[chess.Move, int]


@dataclass
class EngineResult:
    bestmove: str
    ponder: str
    score: str
    pv: str
    nodes: int


def run(cmd: list[str], cwd: Path, *, quiet: bool = False) -> None:
    if not quiet:
        print(f"$ {' '.join(cmd)}  (cwd={cwd})")
    subprocess.run(cmd, cwd=cwd, check=True)


def copy_worktree(src: Path, dst: Path) -> None:
    ignore_names = {
        ".git",
        "target",
        "node_modules",
        "dist",
        ".DS_Store",
    }

    def ignore(_dir: str, names: list[str]) -> set[str]:
        return {name for name in names if name in ignore_names}

    shutil.copytree(src, dst, ignore=ignore)


def export_git_ref(ref: str, dst: Path) -> None:
    dst.mkdir(parents=True, exist_ok=True)
    archive = subprocess.Popen(
        ["git", "archive", "--format=tar", ref], cwd=REPO, stdout=subprocess.PIPE
    )
    try:
        subprocess.run(["tar", "-xf", "-", "-C", str(dst)], stdin=archive.stdout, check=True)
    finally:
        if archive.stdout:
            archive.stdout.close()
        rc = archive.wait()
        if rc != 0:
            raise subprocess.CalledProcessError(rc, ["git", "archive", ref])


def build_engine(src: Path, label: str) -> Path:
    print(f"\n== Building {label} engine ==")
    run(["cargo", "build", "--release"], cwd=src)
    exe = src / "target" / "release" / "boa"
    if not exe.exists():
        raise FileNotFoundError(exe)
    return exe


def epd_op(line: str, name: str) -> str | None:
    m = re.search(rf'(?:^|;)\s*{re.escape(name)}\s+"([^"]*)"\s*(?:;|$)', line)
    if m:
        return m.group(1)
    m = re.search(rf'(?:^|;)\s*{re.escape(name)}\s+([^;]*)\s*(?:;|$)', line)
    return m.group(1).strip() if m else None


def parse_move(board: chess.Board, text: str) -> chess.Move | None:
    text = text.strip()
    if not text:
        return None
    # Common STS decoration. Keep check/mate markers for SAN parser, but remove
    # annotation glyphs and trailing punctuation.
    text = re.sub(r"[!?]+$", "", text)
    try:
        return board.parse_san(text)
    except ValueError:
        pass
    try:
        return chess.Move.from_uci(text.lower())
    except ValueError:
        return None


def parse_epd_file(path: Path) -> list[Position]:
    positions: list[Position] = []
    with path.open("r", encoding="utf-8", errors="replace") as f:
        for line_no, raw in enumerate(f, 1):
            line = raw.strip()
            if not line or line.startswith("#"):
                continue

            fields = line.split()
            if len(fields) < 4:
                continue
            fen = " ".join(fields[:4]) + " 0 1"
            board = chess.Board(fen)
            ident = epd_op(line, "id") or f"{path.stem}:{line_no}"

            bm_moves: set[chess.Move] = set()
            bm_text = epd_op(line, "bm") or ""
            for token in bm_text.split():
                move = parse_move(board, token)
                if move:
                    bm_moves.add(move)

            c0_scores: dict[chess.Move, int] = {}
            c0_text = epd_op(line, "c0") or ""
            for part in c0_text.split(","):
                part = part.strip()
                if "=" not in part:
                    continue
                move_text, score_text = part.rsplit("=", 1)
                try:
                    score = int(score_text.strip())
                except ValueError:
                    continue
                move = parse_move(board, move_text.strip())
                if move:
                    c0_scores[move] = score

            max_score = max(c0_scores.values(), default=10 if bm_moves else 0)
            positions.append(
                Position(
                    suite=path.stem,
                    line_no=line_no,
                    fen=fen,
                    ident=ident,
                    max_score=max_score,
                    bm=bm_moves,
                    c0=c0_scores,
                )
            )
    return positions


def natural_epd_key(path: Path) -> tuple[int, str]:
    m = re.search(r"(\d+)", path.stem)
    return (int(m.group(1)) if m else 9999, path.name)


def load_positions(sts_dir: Path, limit: int) -> list[Position]:
    files = sorted(sts_dir.glob("*.epd"), key=natural_epd_key)
    positions: list[Position] = []
    for file in files:
        positions.extend(parse_epd_file(file))
        if limit and len(positions) >= limit:
            return positions[:limit]
    return positions


class UciEngine:
    def __init__(self, exe: Path, name: str, timeout: float, hash_mb: int):
        self.exe = exe
        self.name = name
        self.timeout = timeout
        self.proc = subprocess.Popen(
            [str(exe)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        self.send("uci")
        self.wait_for("uciok")
        self.send("setoption name Threads value 1")
        self.send(f"setoption name Hash value {hash_mb}")
        self.ready()

    def send(self, cmd: str) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write(cmd + "\n")
        self.proc.stdin.flush()

    def readline(self) -> str:
        assert self.proc.stdout is not None
        line = self.proc.stdout.readline()
        if line == "" and self.proc.poll() is not None:
            raise RuntimeError(f"{self.name} exited with code {self.proc.returncode}")
        return line.rstrip("\n")

    def wait_for(self, marker: str) -> None:
        deadline = time.time() + self.timeout
        while time.time() < deadline:
            if self.readline().strip() == marker:
                return
        raise TimeoutError(f"{self.name}: timeout waiting for {marker}")

    def ready(self) -> None:
        self.send("isready")
        self.wait_for("readyok")

    def search(self, fen: str, depth: int) -> EngineResult:
        self.send(f"position fen {fen}")
        self.send(f"go depth {depth}")
        deadline = time.time() + self.timeout
        score = ""
        pv = ""
        nodes = 0
        while time.time() < deadline:
            line = self.readline().strip()
            if line.startswith("info "):
                sm = re.search(r"\bscore\s+(cp\s+-?\d+|mate\s+-?\d+)", line)
                if sm:
                    score = sm.group(1)
                nm = re.search(r"\bnodes\s+(\d+)", line)
                if nm:
                    nodes = int(nm.group(1))
                pm = re.search(r"\bpv\s+(.+)$", line)
                if pm:
                    pv = pm.group(1)
            elif line.startswith("bestmove "):
                parts = line.split()
                bestmove = parts[1] if len(parts) > 1 else "0000"
                ponder = parts[3] if len(parts) > 3 and parts[2] == "ponder" else ""
                return EngineResult(bestmove, ponder, score, pv, nodes)
        raise TimeoutError(f"{self.name}: timeout on fen {fen}")

    def close(self) -> None:
        if self.proc.poll() is None:
            try:
                self.send("quit")
            except Exception:
                pass
            try:
                self.proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.proc.kill()


def score_move(pos: Position, uci: str) -> tuple[int, bool, str]:
    if uci == "0000":
        return (0, False, "0000")
    board = chess.Board(pos.fen)
    try:
        move = chess.Move.from_uci(uci)
    except ValueError:
        return (0, False, uci)
    san = board.san(move) if move in board.legal_moves else uci
    if move in pos.c0:
        return (pos.c0[move], move in pos.bm, san)
    if move in pos.bm:
        return (10, True, san)
    return (0, False, san)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--sts-dir", default=str(REPO / "STS"), help="directory containing STS*.epd")
    ap.add_argument("--depth", type=int, default=10, help="fixed search depth")
    ap.add_argument("--limit", type=int, default=0, help="limit number of positions, 0 = all")
    ap.add_argument("--baseline-ref", default="HEAD", help="git ref for old plain-futility baseline")
    ap.add_argument("--baseline-engine", type=Path, help="existing baseline engine path; skips baseline build")
    ap.add_argument("--candidate-engine", type=Path, help="existing candidate engine path; skips candidate build")
    ap.add_argument("--out-dir", type=Path, default=REPO / "sts_results", help="result output directory")
    ap.add_argument("--timeout", type=float, default=120.0, help="seconds per engine command/search")
    ap.add_argument("--hash", type=int, default=128, help="UCI hash MB per engine")
    ap.add_argument("--pv-sample", type=int, default=20, help="max differing PV rows in summary")
    args = ap.parse_args()

    sts_dir = Path(args.sts_dir).resolve()
    if not sts_dir.exists():
        print(f"error: STS directory not found: {sts_dir}", file=sys.stderr)
        return 2

    positions = load_positions(sts_dir, args.limit)
    if not positions:
        print(f"error: no EPD positions found in {sts_dir}", file=sys.stderr)
        return 2

    stamp = dt.datetime.now().strftime("%Y%m%d_%H%M%S")
    out_dir = args.out_dir / f"sts_ffp_compare_{stamp}"
    out_dir.mkdir(parents=True, exist_ok=True)
    details_csv = out_dir / "details.csv"
    summary_txt = out_dir / "summary.txt"

    temp_root_ctx = tempfile.TemporaryDirectory(prefix="boa_sts_ffp_")
    temp_root = Path(temp_root_ctx.name)
    baseline_exe = args.baseline_engine
    candidate_exe = args.candidate_engine

    try:
        if baseline_exe is None:
            baseline_src = temp_root / "baseline"
            print(f"Exporting baseline ref {args.baseline_ref} to {baseline_src}")
            export_git_ref(args.baseline_ref, baseline_src)
            baseline_exe = build_engine(baseline_src, "baseline")

        if candidate_exe is None:
            candidate_src = temp_root / "candidate"
            print(f"Copying current working tree to {candidate_src}")
            copy_worktree(REPO, candidate_src)
            candidate_exe = build_engine(candidate_src, "candidate")

        print(f"\n== Running STS comparison: {len(positions)} positions at depth {args.depth} ==")
        print(f"baseline:  {baseline_exe}")
        print(f"candidate: {candidate_exe}")
        print(f"results:   {out_dir}")

        baseline = UciEngine(baseline_exe, "baseline", args.timeout, args.hash)
        candidate = UciEngine(candidate_exe, "candidate", args.timeout, args.hash)

        totals = {
            "base_score": 0,
            "cand_score": 0,
            "base_bm": 0,
            "cand_bm": 0,
            "base_nodes": 0,
            "cand_nodes": 0,
            "regressions": 0,
            "improvements": 0,
            "new_blunders": 0,
            "fixed_blunders": 0,
            "bestmove_diffs": 0,
        }
        pv_diffs: list[dict[str, str]] = []

        with details_csv.open("w", newline="", encoding="utf-8") as f:
            writer = csv.DictWriter(
                f,
                fieldnames=[
                    "suite",
                    "line",
                    "id",
                    "fen",
                    "max_score",
                    "base_best",
                    "base_san",
                    "base_sts_score",
                    "base_bm_hit",
                    "base_engine_score",
                    "base_nodes",
                    "base_pv",
                    "cand_best",
                    "cand_san",
                    "cand_sts_score",
                    "cand_bm_hit",
                    "cand_engine_score",
                    "cand_nodes",
                    "cand_pv",
                    "delta_sts",
                    "new_blunder",
                    "bestmove_diff",
                ],
            )
            writer.writeheader()

            for idx, pos in enumerate(positions, 1):
                br = baseline.search(pos.fen, args.depth)
                cr = candidate.search(pos.fen, args.depth)
                bs, bbm, bsan = score_move(pos, br.bestmove)
                cs, cbm, csan = score_move(pos, cr.bestmove)
                delta = cs - bs
                new_blunder = bs > 0 and cs == 0
                bestmove_diff = br.bestmove != cr.bestmove

                totals["base_score"] += bs
                totals["cand_score"] += cs
                totals["base_bm"] += int(bbm)
                totals["cand_bm"] += int(cbm)
                totals["base_nodes"] += br.nodes
                totals["cand_nodes"] += cr.nodes
                totals["regressions"] += int(delta < 0)
                totals["improvements"] += int(delta > 0)
                totals["new_blunders"] += int(new_blunder)
                totals["fixed_blunders"] += int(bs == 0 and cs > 0)
                totals["bestmove_diffs"] += int(bestmove_diff)

                row = {
                    "suite": pos.suite,
                    "line": pos.line_no,
                    "id": pos.ident,
                    "fen": pos.fen,
                    "max_score": pos.max_score,
                    "base_best": br.bestmove,
                    "base_san": bsan,
                    "base_sts_score": bs,
                    "base_bm_hit": int(bbm),
                    "base_engine_score": br.score,
                    "base_nodes": br.nodes,
                    "base_pv": br.pv,
                    "cand_best": cr.bestmove,
                    "cand_san": csan,
                    "cand_sts_score": cs,
                    "cand_bm_hit": int(cbm),
                    "cand_engine_score": cr.score,
                    "cand_nodes": cr.nodes,
                    "cand_pv": cr.pv,
                    "delta_sts": delta,
                    "new_blunder": int(new_blunder),
                    "bestmove_diff": int(bestmove_diff),
                }
                writer.writerow(row)
                if bestmove_diff and len(pv_diffs) < args.pv_sample:
                    pv_diffs.append({k: str(v) for k, v in row.items()})

                if idx % 25 == 0 or idx == len(positions):
                    print(
                        f"{idx:4d}/{len(positions)}  "
                        f"base={totals['base_score']} cand={totals['cand_score']} "
                        f"new_blunders={totals['new_blunders']} regressions={totals['regressions']}"
                    )

        baseline.close()
        candidate.close()

        max_total = sum(p.max_score for p in positions)
        node_delta_pct = (
            (totals["cand_nodes"] - totals["base_nodes"]) / totals["base_nodes"] * 100.0
            if totals["base_nodes"]
            else 0.0
        )
        verdict = (
            "PASS"
            if totals["new_blunders"] == 0 and totals["cand_score"] >= totals["base_score"]
            else "FAIL"
        )

        with summary_txt.open("w", encoding="utf-8") as f:
            def both(s: str = "") -> None:
                print(s)
                print(s, file=f)

            both("STS CG-FFP comparison")
            both(f"verdict: {verdict}")
            both(f"depth: {args.depth}")
            both(f"positions: {len(positions)}")
            both(f"baseline_ref: {args.baseline_ref}")
            both(f"baseline_engine: {baseline_exe}")
            both(f"candidate_engine: {candidate_exe}")
            both(f"max_sts_score: {max_total}")
            both(f"baseline_sts_score: {totals['base_score']}")
            both(f"candidate_sts_score: {totals['cand_score']}")
            both(f"delta_sts_score: {totals['cand_score'] - totals['base_score']}")
            both(f"baseline_bm_hits: {totals['base_bm']}")
            both(f"candidate_bm_hits: {totals['cand_bm']}")
            both(f"regressions: {totals['regressions']}")
            both(f"improvements: {totals['improvements']}")
            both(f"new_blunders: {totals['new_blunders']}")
            both(f"fixed_blunders: {totals['fixed_blunders']}")
            both(f"bestmove_diffs: {totals['bestmove_diffs']}")
            both(f"baseline_nodes: {totals['base_nodes']}")
            both(f"candidate_nodes: {totals['cand_nodes']}")
            both(f"node_delta_pct: {node_delta_pct:.2f}%")
            both(f"details_csv: {details_csv}")

            if pv_diffs:
                both("\nPV/bestmove difference sample:")
                for row in pv_diffs:
                    both(
                        f"- {row['suite']}:{row['line']} {row['id']} "
                        f"base {row['base_san']}({row['base_sts_score']}) "
                        f"cand {row['cand_san']}({row['cand_sts_score']}) "
                        f"delta {row['delta_sts']}"
                    )
                    both(f"  base pv: {row['base_pv']}")
                    both(f"  cand pv: {row['cand_pv']}")

        print(f"\nWrote: {summary_txt}")
        print(f"Wrote: {details_csv}")
        return 0 if verdict == "PASS" else 1

    finally:
        temp_root_ctx.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())
