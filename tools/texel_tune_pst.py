#!/usr/bin/env python3
"""Tune Boa pawn and knight PST weights from a self-play feature CSV.

This is the first internal-weight Texel slice. It keeps all non-pawn/knight PST
terms and every non-PST term fixed, then coordinate-tunes the pawn and knight
midgame/endgame PST entries against game-result labels.
"""

from __future__ import annotations

import argparse
import csv
import math
from dataclasses import dataclass


PST_PAWN = [
    (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0),
    (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0),
    (5, 5), (5, 5), (10, 10), (0, 0), (0, 0), (10, 10), (5, 5), (5, 5),
    (5, 5), (10, 10), (15, 15), (25, 25), (25, 25), (15, 15), (10, 10), (5, 5),
    (10, 10), (15, 15), (20, 20), (30, 30), (30, 30), (20, 20), (15, 15), (10, 10),
    (20, 20), (25, 25), (30, 30), (35, 35), (35, 35), (30, 30), (25, 25), (20, 20),
    (40, 50), (45, 55), (45, 55), (45, 55), (45, 55), (45, 55), (45, 55), (40, 50),
    (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0), (0, 0),
]

PST_KNIGHT = [
    (-50, -30), (-40, -20), (-30, -10), (-30, -10), (-30, -10), (-30, -10), (-40, -20), (-50, -30),
    (-40, -20), (-20, -5), (0, 0), (0, 0), (0, 0), (0, 0), (-20, -5), (-40, -20),
    (-30, -10), (0, 0), (10, 5), (15, 10), (15, 10), (10, 5), (0, 0), (-30, -10),
    (-30, -10), (5, 5), (15, 10), (20, 15), (20, 15), (15, 10), (5, 5), (-30, -10),
    (-30, -10), (0, 0), (15, 10), (20, 15), (20, 15), (15, 10), (0, 0), (-30, -10),
    (-30, -10), (5, 5), (10, 5), (15, 10), (15, 10), (10, 5), (5, 5), (-30, -10),
    (-40, -20), (-20, -5), (0, 0), (5, 5), (5, 5), (0, 0), (-20, -5), (-40, -20),
    (-50, -30), (-40, -20), (-30, -10), (-30, -10), (-30, -10), (-30, -10), (-40, -20), (-50, -30),
]

PIECE_TO_TABLE = {
    "P": ("pawn", 1),
    "N": ("knight", 1),
    "p": ("pawn", -1),
    "n": ("knight", -1),
}


@dataclass
class Dataset:
    labels: list[float]
    evals: list[float]
    coeffs: list[dict[int, float]]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "csv_path",
        nargs="?",
        default="analysis/self_play/texel_self_play.csv",
        help="Self-play CSV from tools/self_play_dataset.mjs.",
    )
    parser.add_argument("--limit", type=int, default=0, help="Maximum rows to read. Default: all.")
    parser.add_argument("--k", type=float, default=0.004, help="Sigmoid slope. Default: 0.004.")
    parser.add_argument(
        "--steps",
        default="8,4,2,1",
        help="Coordinate descent step sizes in centipawns. Default: 8,4,2,1.",
    )
    parser.add_argument("--min-value", type=int, default=-120)
    parser.add_argument("--max-value", type=int, default=120)
    parser.add_argument("--passes", type=int, default=2, help="Max passes per step. Default: 2.")
    return parser.parse_args()


def split_csv_arg(value: str) -> list[int]:
    return [int(item.strip()) for item in value.split(",") if item.strip()]


def sigmoid(k: float, cp: float) -> float:
    x = max(-60.0, min(60.0, k * cp))
    return 1.0 / (1.0 + math.exp(-x))


def row_error(label: float, cp: float, k: float) -> float:
    diff = sigmoid(k, cp) - label
    return diff * diff


def label_from_result(value: str) -> float | None:
    if value == "":
        return None
    try:
        score = float(value)
    except ValueError:
        return None
    if score > 0:
        return 1.0
    if score < 0:
        return 0.0
    return 0.5


def parse_fen_board(fen: str) -> list[tuple[str, int]]:
    board = fen.split()[0]
    pieces: list[tuple[str, int]] = []
    rank = 7
    file = 0
    for ch in board:
        if ch == "/":
            rank -= 1
            file = 0
            continue
        if ch.isdigit():
            file += int(ch)
            continue
        pieces.append((ch, rank * 8 + file))
        file += 1
    return pieces


def orient_square(square: int, sign: int) -> int:
    if sign > 0:
        return square
    rank = square // 8
    file = square % 8
    return (7 - rank) * 8 + file


def param_index(piece_name: str, square: int, phase_name: str) -> int:
    piece_offset = 0 if piece_name == "pawn" else 128
    phase_offset = 0 if phase_name == "mg" else 64
    return piece_offset + phase_offset + square


def default_params() -> list[int]:
    values: list[int] = []
    for table in (PST_PAWN, PST_KNIGHT):
        values.extend(mg for mg, _eg in table)
        values.extend(eg for _mg, eg in table)
    return values


def row_coeffs(fen: str, phase: int) -> dict[int, float]:
    coeffs: dict[int, float] = {}
    mg_weight = phase / 256.0
    eg_weight = (256 - phase) / 256.0
    for piece, square in parse_fen_board(fen):
        mapped = PIECE_TO_TABLE.get(piece)
        if mapped is None:
            continue
        piece_name, sign = mapped
        oriented = orient_square(square, sign)
        mg_idx = param_index(piece_name, oriented, "mg")
        eg_idx = param_index(piece_name, oriented, "eg")
        coeffs[mg_idx] = coeffs.get(mg_idx, 0.0) + sign * mg_weight
        coeffs[eg_idx] = coeffs.get(eg_idx, 0.0) + sign * eg_weight
    return coeffs


def contribution(params: list[int], coeffs: dict[int, float]) -> float:
    return sum(params[index] * coeff for index, coeff in coeffs.items())


def read_dataset(path: str, limit: int) -> Dataset:
    params = default_params()
    labels: list[float] = []
    evals: list[float] = []
    coeffs_by_param: list[dict[int, float]] = [dict() for _ in params]

    with open(path, newline="", encoding="utf8") as handle:
        reader = csv.DictReader(handle)
        required = {"fen", "phase", "white_score_cp", "result_score"}
        missing = sorted(required - set(reader.fieldnames or []))
        if missing:
            raise SystemExit(f"Missing required columns: {', '.join(missing)}")

        for row in reader:
            label = label_from_result(row.get("result_score", ""))
            if label is None:
                continue
            try:
                phase = int(row["phase"])
                white_score = float(row["white_score_cp"])
            except ValueError:
                continue

            coeffs = row_coeffs(row["fen"], phase)
            base_without_tuned_pst = white_score - contribution(params, coeffs)
            row_index = len(labels)
            labels.append(label)
            evals.append(base_without_tuned_pst + contribution(params, coeffs))

            for param, coeff in coeffs.items():
                coeffs_by_param[param][row_index] = coeff

            if limit > 0 and len(labels) >= limit:
                break

    if not labels:
        raise SystemExit("No usable rows found.")
    return Dataset(labels=labels, evals=evals, coeffs=coeffs_by_param)


def error_sum(labels: list[float], evals: list[float], k: float) -> float:
    return sum(row_error(label, cp, k) for label, cp in zip(labels, evals))


def try_delta(
    dataset: Dataset,
    errors: list[float],
    param: int,
    delta: int,
    k: float,
) -> tuple[float, list[tuple[int, float, float]]] | None:
    changed: list[tuple[int, float, float]] = []
    delta_error = 0.0
    for row, coeff in dataset.coeffs[param].items():
        old_eval = dataset.evals[row]
        new_eval = old_eval + coeff * delta
        new_error = row_error(dataset.labels[row], new_eval, k)
        delta_error += new_error - errors[row]
        changed.append((row, new_eval, new_error))
    if delta_error < -1e-12:
        return delta_error, changed
    return None


def tune(dataset: Dataset, args: argparse.Namespace) -> tuple[list[int], float, float]:
    params = default_params()
    errors = [row_error(label, cp, args.k) for label, cp in zip(dataset.labels, dataset.evals)]
    current_error_sum = sum(errors)
    initial_mse = current_error_sum / len(dataset.labels)

    for step in split_csv_arg(args.steps):
        for _ in range(args.passes):
            improved = False
            for index in range(len(params)):
                best = None
                current = params[index]
                for direction in (1, -1):
                    candidate = max(args.min_value, min(args.max_value, current + direction * step))
                    if candidate == current:
                        continue
                    attempt = try_delta(dataset, errors, index, candidate - current, args.k)
                    if attempt is not None and (best is None or attempt[0] < best[0]):
                        best = (attempt[0], candidate, attempt[1])

                if best is None:
                    continue

                delta_error, candidate, changed = best
                params[index] = candidate
                current_error_sum += delta_error
                for row, new_eval, new_error in changed:
                    dataset.evals[row] = new_eval
                    errors[row] = new_error
                improved = True

            if not improved:
                break

    return params, initial_mse, current_error_sum / len(dataset.labels)


def table_from_params(params: list[int], offset: int) -> list[tuple[int, int]]:
    mg = params[offset: offset + 64]
    eg = params[offset + 64: offset + 128]
    return list(zip(mg, eg))


def print_rust_table(name: str, table: list[tuple[int, int]]) -> None:
    print(f"\n{name}:")
    for rank in range(8):
        row = table[rank * 8: (rank + 1) * 8]
        print("    " + ",".join(f"({mg:>3},{eg:>3})" for mg, eg in row) + ",")


def print_changed(name: str, before: list[tuple[int, int]], after: list[tuple[int, int]]) -> None:
    changes = []
    for index, (old, new) in enumerate(zip(before, after)):
        if old != new:
            changes.append((index, old, new))
    print(f"{name}_changed={len(changes)}")
    for index, old, new in changes[:32]:
        print(f"{name}[{index}] {old} -> {new}")
    if len(changes) > 32:
        print(f"... {len(changes) - 32} more")


def main() -> None:
    args = parse_args()
    dataset = read_dataset(args.csv_path, args.limit)
    params, initial_mse, best_mse = tune(dataset, args)
    pawn = table_from_params(params, 0)
    knight = table_from_params(params, 128)

    print(f"rows={len(dataset.labels)}")
    print(f"k={args.k}")
    print(f"initial_mse={initial_mse:.8f}")
    print(f"best_mse={best_mse:.8f}")
    print(f"delta_mse={initial_mse - best_mse:.8f}")
    print_changed("pawn", PST_PAWN, pawn)
    print_changed("knight", PST_KNIGHT, knight)
    print_rust_table("PST_PAWN", pawn)
    print_rust_table("PST_KNIGHT", knight)


if __name__ == "__main__":
    main()
