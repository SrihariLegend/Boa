#!/usr/bin/env python3
"""Convert Boa criticality CSV rows to Parquet.

Supports both modes used by the tooling:
  * batch conversion from a raw directory containing criticality-*.csv shards
  * streaming conversion from a CSV file/FIFO/stdin into Parquet part files
"""

from __future__ import annotations

import argparse
import csv
import gzip
import hashlib
import sys
from pathlib import Path
from typing import Iterable, TextIO


INT_COLUMNS = {
    "schema_version",
    "pid",
    "game_id",
    "search_id",
    "root_depth",
    "ply",
    "node_hash",
    "from",
    "to",
    "depth",
    "move_index",
    "base_reduction",
    "final_reduction",
    "new_depth",
    "history_score",
    "static_eval",
    "has_prev_static_eval",
    "prev_static_eval",
    "static_eval_delta",
    "alpha",
    "beta",
    "futility_margin",
    "static_alpha_margin",
    "is_pv",
    "is_cut_node",
    "improving",
    "is_killer",
    "is_counter",
    "tt_move_agreement",
    "reduced_score",
    "full_score",
    "score_delta_cp",
    "bound_changed",
    "regret_cp",
    "sample_permille",
    "node_type",
    "improving",
    "volatility",
    "king_danger",
    "planned_reduction",
    "planned_margin",
    "gap",
    "has_move_index",
    "has_history",
    "has_reduction",
    "has_margin",
    "has_gap",
    "pruned_score",
    "node_count",
}

FLOAT_COLUMNS = {"phase"}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", help="raw CSV dir, CSV file, FIFO, or '-' for stdin")
    parser.add_argument(
        "out",
        nargs="?",
        default="analysis/criticality/latest/criticality.parquet",
        help="Output .parquet file or Parquet dataset directory",
    )
    parser.add_argument("--stream", action="store_true", help="Read one CSV stream and write part files")
    parser.add_argument("--part-prefix", default="part-", help="Prefix for streamed/batched part files")
    parser.add_argument("--batch-rows", type=int, default=100_000, help="Rows per Parquet part in stream mode")
    args = parser.parse_args()

    try:
        import pyarrow as pa
        import pyarrow.parquet as pq
    except ImportError as exc:
        raise SystemExit("pyarrow is required: python3 -m pip install pyarrow") from exc

    if args.batch_rows < 1:
        raise SystemExit("--batch-rows must be positive")

    out_path = Path(args.out)
    if args.stream:
        out_path.mkdir(parents=True, exist_ok=True)
        with open_input(args.input) as handle:
            rows = typed_rows(csv.DictReader(handle))
            count = write_parts(rows, out_path, args.part_prefix, args.batch_rows, pa, pq)
        print(f"wrote {count} streamed rows to {out_path}")
        return

    input_path = Path(args.input)
    if input_path.is_dir():
        shards = sorted(input_path.glob("criticality-*.csv")) + sorted(input_path.glob("criticality-*.csv.gz"))
        if not shards:
            raise SystemExit(f"no criticality-*.csv[.gz] files found in {input_path}")
        rows = iter_shard_rows(shards)
    else:
        rows = rows_from_file(input_path)

    if out_path.suffix == ".parquet":
        batch = list(rows)
        if not batch:
            raise SystemExit(f"no data rows found in {input_path}")
        out_path.parent.mkdir(parents=True, exist_ok=True)
        pq.write_table(pa.Table.from_pylist(batch), out_path)
        print(f"wrote {len(batch)} rows to {out_path}")
    else:
        out_path.mkdir(parents=True, exist_ok=True)
        count = write_parts(rows, out_path, args.part_prefix, args.batch_rows, pa, pq)
        if count == 0:
            raise SystemExit(f"no data rows found in {input_path}")
        print(f"wrote {count} rows to {out_path}")


def open_input(input_name: str) -> TextIO:
    if input_name == "-":
        return sys.stdin
    path = Path(input_name)
    if path.name.endswith(".gz"):
        return gzip.open(path, "rt", newline="", encoding="utf8", errors="replace")
    return path.open(newline="", encoding="utf8", errors="replace")


def rows_from_file(path: Path) -> Iterable[dict[str, object]]:
    with open_input(str(path)) as handle:
        yield from typed_rows(csv.DictReader(handle))


def iter_shard_rows(shards: list[Path]) -> Iterable[dict[str, object]]:
    for shard in shards:
        yield from rows_from_file(shard)


def typed_rows(reader: csv.DictReader[str]) -> Iterable[dict[str, object]]:
    for row_index, row in enumerate(reader):
        typed = type_row(row)
        typed["split"] = split_for(typed.get("pid"), typed.get("game_id"), typed.get("search_id"), row_index)
        yield typed


def type_row(row: dict[str, str]) -> dict[str, object]:
    typed: dict[str, object] = {}
    for key, value in row.items():
        if key in INT_COLUMNS:
            typed[key] = int(value) if value != "" else None
        elif key in FLOAT_COLUMNS:
            typed[key] = float(value) if value != "" else None
        else:
            typed[key] = value if value != "" else None
    return typed


def write_parts(rows: Iterable[dict[str, object]], out_dir: Path, prefix: str, batch_rows: int, pa, pq) -> int:
    batch: list[dict[str, object]] = []
    total = 0
    part = 0
    for row in rows:
        batch.append(row)
        if len(batch) >= batch_rows:
            write_part(batch, out_dir, prefix, part, pa, pq)
            total += len(batch)
            part += 1
            batch = []
    if batch:
        write_part(batch, out_dir, prefix, part, pa, pq)
        total += len(batch)
    return total


def write_part(rows: list[dict[str, object]], out_dir: Path, prefix: str, part: int, pa, pq) -> None:
    path = out_dir / f"{prefix}{part:06d}.parquet"
    pq.write_table(pa.Table.from_pylist(rows), path)


def split_for(*parts: object) -> str:
    digest = hashlib.blake2b(":".join(str(part) for part in parts).encode("ascii"), digest_size=4).digest()
    bucket = int.from_bytes(digest, "little") % 10
    if bucket == 0:
        return "test"
    if bucket == 1:
        return "validation"
    return "train"


if __name__ == "__main__":
    main()
