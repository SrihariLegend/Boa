#!/usr/bin/env python3
"""Summarize unified criticality probe logs without printing row data.

Reads `.csv` and `.csv.gz` files as a stream and emits only bounded aggregate
metrics: file sizes, row counts, per-kind counts, error rates, and average probe
subtree nodes. This is intentionally safe for large logs: it never materializes
rows and never dumps CSV contents.
"""

from __future__ import annotations

import argparse
import csv
import gzip
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, TextIO


@dataclass
class KindStats:
    rows: int = 0
    bound_changed: int = 0
    node_count_sum: int = 0
    score_delta_sum: int = 0
    regret_sum: int = 0

    def add(self, bound_changed: bool, node_count: int, score_delta: int, regret: int) -> None:
        self.rows += 1
        self.bound_changed += int(bound_changed)
        self.node_count_sum += node_count
        self.score_delta_sum += score_delta
        self.regret_sum += regret


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "paths",
        nargs="+",
        help="Probe .csv/.csv.gz files or directories containing them.",
    )
    parser.add_argument(
        "--max-rows",
        type=int,
        default=0,
        help="Stop after this many data rows total; 0 means no row cap.",
    )
    return parser.parse_args()


def iter_files(paths: Iterable[str]) -> list[Path]:
    files: list[Path] = []
    for raw in paths:
        path = Path(raw)
        if path.is_dir():
            files.extend(sorted(path.glob("*.csv")))
            files.extend(sorted(path.glob("*.csv.gz")))
        elif path.is_file():
            files.append(path)
        else:
            raise SystemExit(f"missing path: {path}")
    return sorted(set(files))


def open_text(path: Path) -> TextIO:
    if path.name.endswith(".gz"):
        return gzip.open(path, "rt", newline="")
    return path.open("rt", newline="")


def parse_int(value: str | None, default: int = 0) -> int:
    if not value:
        return default
    try:
        return int(value)
    except ValueError:
        return default


def parse_bool01(value: str | None) -> bool:
    return value in {"1", "true", "True", "TRUE"}


def main() -> int:
    args = parse_args()
    files = iter_files(args.paths)
    if not files:
        print("no probe csv files found")
        return 1

    total_compressed_bytes = sum(path.stat().st_size for path in files)
    total_rows = 0
    kind_stats: dict[str, KindStats] = defaultdict(KindStats)
    node_types: Counter[str] = Counter()
    stopped_by_cap = False

    for path in files:
        with open_text(path) as handle:
            reader = csv.DictReader(handle)
            required = {"decision_kind", "bound_changed", "node_count", "score_delta"}
            if not reader.fieldnames or not required.issubset(reader.fieldnames):
                raise SystemExit(f"unsupported probe schema in {path}")

            for row in reader:
                kind = row.get("decision_kind") or "<missing>"
                node_count = parse_int(row.get("node_count"))
                score_delta = parse_int(row.get("score_delta"))
                regret = parse_int(row.get("regret_cp"))
                changed = parse_bool01(row.get("bound_changed"))
                kind_stats[kind].add(changed, node_count, score_delta, regret)
                node_types[row.get("node_type") or "<missing>"] += 1
                total_rows += 1

                if args.max_rows and total_rows >= args.max_rows:
                    stopped_by_cap = True
                    break
        if stopped_by_cap:
            break

    print(f"files: {len(files)}")
    print(f"compressed_bytes: {total_compressed_bytes}")
    print(f"rows: {total_rows}" + (" (capped)" if stopped_by_cap else ""))
    print("decision_kind,count,bound_changed,rate,avg_node_count,avg_score_delta,avg_regret_cp")
    for kind in sorted(kind_stats):
        stats = kind_stats[kind]
        rate = stats.bound_changed / stats.rows if stats.rows else 0.0
        avg_nodes = stats.node_count_sum / stats.rows if stats.rows else 0.0
        avg_delta = stats.score_delta_sum / stats.rows if stats.rows else 0.0
        avg_regret = stats.regret_sum / stats.rows if stats.rows else 0.0
        print(
            f"{kind},{stats.rows},{stats.bound_changed},"
            f"{rate:.6f},{avg_nodes:.2f},{avg_delta:.2f},{avg_regret:.2f}"
        )
    print("node_type_counts:" + ",".join(f"{k}={node_types[k]}" for k in sorted(node_types)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
