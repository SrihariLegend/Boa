#!/usr/bin/env python3
"""Convert Boa criticality CSV shards to one Parquet dataset."""

from __future__ import annotations

import argparse
import csv
import hashlib
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("raw_dir", help="Directory containing criticality-*.csv shards")
    parser.add_argument(
        "out",
        nargs="?",
        default="analysis/criticality/latest/criticality.parquet",
        help="Output Parquet path",
    )
    args = parser.parse_args()

    try:
        import pyarrow as pa
        import pyarrow.parquet as pq
    except ImportError as exc:
        raise SystemExit("pyarrow is required: python3 -m pip install pyarrow") from exc

    raw_dir = Path(args.raw_dir)
    out_path = Path(args.out)
    shards = sorted(raw_dir.glob("criticality-*.csv"))
    if not shards:
        raise SystemExit(f"no criticality-*.csv files found in {raw_dir}")

    rows: list[dict[str, object]] = []
    for shard in shards:
        with shard.open(newline="", encoding="utf8") as handle:
            reader = csv.DictReader(handle)
            for row in reader:
                typed = type_row(row)
                typed["split"] = split_for(typed["pid"], typed["game_id"])
                rows.append(typed)

    if not rows:
        raise SystemExit(f"no data rows found in {raw_dir}")

    out_path.parent.mkdir(parents=True, exist_ok=True)
    table = pa.Table.from_pylist(rows)
    pq.write_table(table, out_path)
    print(f"wrote {len(rows)} rows to {out_path}")


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
}


def type_row(row: dict[str, str]) -> dict[str, object]:
    typed: dict[str, object] = {}
    for key, value in row.items():
        if key in INT_COLUMNS:
            typed[key] = int(value) if value != "" else None
        else:
            typed[key] = value
    return typed


def split_for(pid: object, game_id: object) -> str:
    digest = hashlib.blake2b(f"{pid}:{game_id}".encode("ascii"), digest_size=4).digest()
    bucket = int.from_bytes(digest, "little") % 10
    if bucket == 0:
        return "test"
    if bucket == 1:
        return "validation"
    return "train"


if __name__ == "__main__":
    main()
