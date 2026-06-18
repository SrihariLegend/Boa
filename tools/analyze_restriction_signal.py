#!/usr/bin/env python3
"""Analyze Boa restriction-signal CSVs without third-party dependencies."""

from __future__ import annotations

import argparse
import csv
import math
from dataclasses import dataclass


CONTROL_FEATURES = [
    "mobility_white",
    "mobility_black",
    "pawn_structure_mg",
    "pawn_structure_eg",
    "king_safety_mg",
    "king_safety_eg",
]

RESTRICTION_FEATURES = [
    "white_pawn_breaks",
    "black_pawn_breaks",
    "liberating_breaks_white",
    "liberating_breaks_black",
    "piece_redeployment_white",
    "piece_redeployment_black",
]


@dataclass
class Regression:
    features: list[str]
    beta: list[float]
    stderr: list[float]
    r2: float
    n: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "csv_path",
        nargs="?",
        default="analysis/restriction_signal/gm_features.csv",
        help="Feature CSV from tools/restriction_signal.mjs",
    )
    parser.add_argument(
        "--target",
        default="future_white_score_cp",
        help="Regression target column. Default: future_white_score_cp",
    )
    return parser.parse_args()


def read_rows(path: str) -> list[dict[str, str]]:
    with open(path, newline="", encoding="utf8") as handle:
        return list(csv.DictReader(handle))


def number(row: dict[str, str], key: str) -> float | None:
    value = row.get(key, "")
    if value == "":
        return None
    try:
        parsed = float(value)
    except ValueError:
        return None
    if not math.isfinite(parsed):
        return None
    return parsed


def clean_matrix(
    rows: list[dict[str, str]], target: str, features: list[str]
) -> tuple[list[list[float]], list[float]]:
    x: list[list[float]] = []
    y: list[float] = []
    for row in rows:
        target_value = number(row, target)
        values = [number(row, feature) for feature in features]
        if target_value is None or any(value is None for value in values):
            continue
        x.append([1.0] + [float(value) for value in values if value is not None])
        y.append(target_value)
    return x, y


def transpose(matrix: list[list[float]]) -> list[list[float]]:
    return [list(col) for col in zip(*matrix)]


def matmul(a: list[list[float]], b: list[list[float]]) -> list[list[float]]:
    return [
        [sum(a_row[k] * b[k][j] for k in range(len(b))) for j in range(len(b[0]))]
        for a_row in a
    ]


def matvec(a: list[list[float]], v: list[float]) -> list[float]:
    return [sum(a_row[i] * v[i] for i in range(len(v))) for a_row in a]


def invert(matrix: list[list[float]]) -> list[list[float]]:
    n = len(matrix)
    aug = [
        [*matrix[i], *[1.0 if i == j else 0.0 for j in range(n)]]
        for i in range(n)
    ]
    for col in range(n):
        pivot = max(range(col, n), key=lambda row: abs(aug[row][col]))
        if abs(aug[pivot][col]) < 1e-12:
            aug[pivot][col] += 1e-8
        aug[col], aug[pivot] = aug[pivot], aug[col]
        scale = aug[col][col]
        aug[col] = [value / scale for value in aug[col]]
        for row in range(n):
            if row == col:
                continue
            factor = aug[row][col]
            aug[row] = [
                aug[row][i] - factor * aug[col][i] for i in range(2 * n)
            ]
    return [row[n:] for row in aug]


def regress(rows: list[dict[str, str]], target: str, features: list[str]) -> Regression:
    x, y = clean_matrix(rows, target, features)
    if len(x) <= len(features) + 2:
        raise SystemExit("Not enough complete rows for regression.")

    xt = transpose(x)
    xtx = matmul(xt, x)
    xtx_inv = invert(xtx)
    beta = matvec(xtx_inv, matvec(xt, y))
    pred = matvec(x, beta)
    mean_y = sum(y) / len(y)
    sse = sum((actual - estimate) ** 2 for actual, estimate in zip(y, pred))
    sst = sum((actual - mean_y) ** 2 for actual in y)
    dof = max(1, len(y) - len(beta))
    sigma2 = sse / dof
    stderr = [math.sqrt(max(0.0, sigma2 * xtx_inv[i][i])) for i in range(len(beta))]
    return Regression(["intercept", *features], beta, stderr, 1.0 - sse / sst, len(y))


def print_regression(title: str, regression: Regression) -> None:
    print(f"\n{title}")
    print(f"n={regression.n} r2={regression.r2:.4f}")
    print("feature,coef,stderr,t")
    for feature, coef, stderr in zip(regression.features, regression.beta, regression.stderr):
        t_stat = coef / stderr if stderr else 0.0
        print(f"{feature},{coef:.6f},{stderr:.6f},{t_stat:.3f}")


def equal_mobility_zero_break_summary(rows: list[dict[str, str]]) -> None:
    buckets: dict[tuple[int, int], list[dict[str, str]]] = {}
    for row in rows:
        mw = number(row, "mobility_white")
        mb = number(row, "mobility_black")
        if mw is None or mb is None:
            continue
        buckets.setdefault((int(mw), int(mb)), []).append(row)

    zero_scores: list[float] = []
    nonzero_scores: list[float] = []
    for bucket in buckets.values():
        if len(bucket) < 4:
            continue
        for row in bucket:
            target = number(row, "future_white_score_cp")
            white_breaks = number(row, "liberating_breaks_white")
            black_breaks = number(row, "liberating_breaks_black")
            if target is None or white_breaks is None or black_breaks is None:
                continue
            if white_breaks == 0 and black_breaks == 0:
                zero_scores.append(target)
            else:
                nonzero_scores.append(target)

    print("\nEqual-mobility zero-break comparison")
    if not zero_scores or not nonzero_scores:
        print("Not enough paired equal-mobility rows for a useful comparison.")
        return
    print(f"zero_break_rows={len(zero_scores)} mean_future_white_cp={mean(zero_scores):.2f}")
    print(f"nonzero_break_rows={len(nonzero_scores)} mean_future_white_cp={mean(nonzero_scores):.2f}")


def mean(values: list[float]) -> float:
    return sum(values) / len(values)


def main() -> None:
    args = parse_args()
    rows = read_rows(args.csv_path)
    print(f"Loaded {len(rows)} rows from {args.csv_path}")

    base = regress(rows, args.target, CONTROL_FEATURES)
    full = regress(rows, args.target, CONTROL_FEATURES + RESTRICTION_FEATURES)
    print_regression("Controls only", base)
    print_regression("Controls + restriction candidates", full)
    print(f"\nrestriction_delta_r2={full.r2 - base.r2:.6f}")
    equal_mobility_zero_break_summary(rows)


if __name__ == "__main__":
    main()
