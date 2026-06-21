#!/usr/bin/env python3
"""Extract labeled criticality examples from large Boa CSV shards.

This is intentionally streaming and stdlib-only: it can reduce multi-GB raw
criticality logs to a small supervised dataset without loading the raw logs
into memory. Rows with label_source=none are skipped by default.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("raw_dir", help="Directory containing criticality-*.csv shards")
    parser.add_argument("out", help="Output labeled CSV path")
    parser.add_argument(
        "--include-unlabeled-permille",
        type=int,
        default=0,
        help="Also include a deterministic sample of unlabeled rows in [0,1000]",
    )
    args = parser.parse_args()

    raw_dir = Path(args.raw_dir)
    out_path = Path(args.out)
    shards = sorted(raw_dir.glob("criticality-*.csv"))
    if not shards:
        raise SystemExit(f"no criticality-*.csv files found in {raw_dir}")
    sample_permille = max(0, min(1000, args.include_unlabeled_permille))

    out_path.parent.mkdir(parents=True, exist_ok=True)
    total = written = labeled = sampled_unlabeled = 0
    fieldnames: list[str] | None = None

    with out_path.open("w", newline="", encoding="utf8") as out_handle:
        writer: csv.DictWriter[str] | None = None
        for shard in shards:
            with shard.open(newline="", encoding="utf8", errors="replace") as handle:
                reader = csv.DictReader(handle)
                if reader.fieldnames is None:
                    continue
                if fieldnames is None:
                    fieldnames = list(reader.fieldnames)
                    if "split" not in fieldnames:
                        fieldnames.append("split")
                    writer = csv.DictWriter(out_handle, fieldnames=fieldnames)
                    writer.writeheader()

                for row in reader:
                    total += 1
                    is_labeled = row.get("label_source") not in ("", "none", None)
                    if is_labeled:
                        labeled += 1
                    elif sample_permille == 0 or sample_bucket(row) >= sample_permille:
                        continue
                    else:
                        sampled_unlabeled += 1

                    row["split"] = split_for(row.get("pid", ""), row.get("game_id", ""))
                    assert writer is not None
                    writer.writerow(row)
                    written += 1

    if written == 0:
        raise SystemExit(f"no rows written from {raw_dir}")
    print(
        f"read {total} rows; wrote {written} rows "
        f"({labeled} labeled, {sampled_unlabeled} sampled_unlabeled) to {out_path}"
    )


def split_for(pid: object, game_id: object) -> str:
    digest = hashlib.blake2b(f"{pid}:{game_id}".encode("ascii"), digest_size=4).digest()
    bucket = int.from_bytes(digest, "little") % 10
    if bucket == 0:
        return "test"
    if bucket == 1:
        return "validation"
    return "train"


def sample_bucket(row: dict[str, str]) -> int:
    key = ":".join(
        [
            row.get("pid", ""),
            row.get("game_id", ""),
            row.get("search_id", ""),
            row.get("node_hash", ""),
            row.get("move_uci", ""),
        ]
    )
    digest = hashlib.blake2b(key.encode("utf8"), digest_size=4).digest()
    return int.from_bytes(digest, "little") % 1000


if __name__ == "__main__":
    main()
