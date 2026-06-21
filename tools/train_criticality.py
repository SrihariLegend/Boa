#!/usr/bin/env python3
"""Train/evaluate a lightweight logistic criticality model.

The script is dependency-light on purpose: only numpy is required. It can read
either a labeled CSV created by extract_criticality_labels.py or a raw directory
of criticality-*.csv shards, filtering to rows with label_source != none.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
from pathlib import Path

import numpy as np


FEATURES_FULL = [
    "root_depth",
    "ply",
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
    "side_to_move_black",
    "piece_p",
    "piece_n",
    "piece_b",
    "piece_r",
    "piece_q",
    "piece_k",
]

FEATURES_HISTORY_ONLY = ["history_score"]

SCORE_CLIP = 2000.0
HISTORY_CLIP = 16384.0
SCORE_PERCENTILES = [50.0, 75.0, 90.0, 95.0, 97.0, 99.0, 99.5]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("data", help="Labeled CSV file or raw directory of criticality-*.csv shards")
    parser.add_argument("--epochs", type=int, default=250)
    parser.add_argument("--lr", type=float, default=0.03)
    parser.add_argument("--l2", type=float, default=1e-4)
    parser.add_argument(
        "--probe-permille",
        type=float,
        default=5.0,
        help="Counterfactual probe rate used during collection; used for inverse-probability weights",
    )
    parser.add_argument("--max-rows", type=int, default=0, help="Optional deterministic cap on labeled rows")
    parser.add_argument("--out", help="Optional JSON output for model weights/metrics")
    args = parser.parse_args()

    rows = load_rows(Path(args.data), args.max_rows)
    if not rows:
        raise SystemExit("no labeled rows found")

    print(f"loaded_labeled_rows {len(rows)}")
    print_counts("split", rows)
    print_counts("label_source", rows)
    print_counts("bound_changed", rows)

    y = np.array([int(row["bound_changed"]) for row in rows], dtype=np.float64)
    weights = inclusion_weights(rows, args.probe_permille)
    splits = np.array([row.get("split") or split_for(row.get("pid", ""), row.get("game_id", "")) for row in rows])
    sources = np.array([row.get("label_source", "") for row in rows])

    train_mask = splits == "train"
    val_mask = splits == "validation"
    test_mask = splits == "test"
    if train_mask.sum() == 0 or val_mask.sum() == 0 or test_mask.sum() == 0:
        raise SystemExit(
            f"empty split: train={train_mask.sum()} validation={val_mask.sum()} test={test_mask.sum()}"
        )

    results = {}
    for name, features in [
        ("history_only", FEATURES_HISTORY_ONLY),
        ("full", FEATURES_FULL),
    ]:
        X = build_matrix(rows, features)
        model = fit_logreg(
            X[train_mask],
            y[train_mask],
            weights[train_mask],
            epochs=args.epochs,
            lr=args.lr,
            l2=args.l2,
        )
        logits = decision_logreg(model, X)
        pred = sigmoid(logits)
        platt = fit_platt(
            logits[val_mask],
            y[val_mask],
            weights[val_mask],
            epochs=args.epochs,
            lr=args.lr,
            l2=args.l2,
        )
        pred_platt = apply_platt(platt, logits)

        metrics = build_result_metrics(
            features,
            model,
            y,
            weights,
            sources,
            train_mask,
            val_mask,
            test_mask,
            pred,
            logits,
        )
        metrics["platt"] = platt_to_json(platt)
        calibrated_logits = platt["intercept"] + platt["slope"] * logits
        metrics["calibrated"] = build_result_metrics(
            features,
            model,
            y,
            weights,
            sources,
            train_mask,
            val_mask,
            test_mask,
            pred_platt,
            calibrated_logits,
            platt,
        )
        results[name] = metrics
        print_model_summary(name, metrics)
        print_model_summary(f"{name}_platt", metrics["calibrated"])

    delta_auc = results["full"]["test"]["auc"] - results["history_only"]["test"]["auc"]
    delta_auc_platt = (
        results["full"]["calibrated"]["test"]["auc"]
        - results["history_only"]["calibrated"]["test"]["auc"]
    )
    print(f"test_auc_delta_full_minus_history {delta_auc:.6f}")
    print(f"test_auc_delta_full_platt_minus_history_platt {delta_auc_platt:.6f}")

    if args.out:
        out_path = Path(args.out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(json.dumps(results, indent=2), encoding="utf8")
        print(f"wrote {out_path}")


def build_result_metrics(
    features: list[str],
    model: dict[str, np.ndarray],
    y: np.ndarray,
    weights: np.ndarray,
    sources: np.ndarray,
    train_mask: np.ndarray,
    val_mask: np.ndarray,
    test_mask: np.ndarray,
    pred: np.ndarray,
    score: np.ndarray,
    platt: dict[str, float] | None = None,
) -> dict[str, object]:
    weights_json = model_to_weights(model, features)
    if platt is not None:
        weights_json = apply_platt_to_raw_weights(weights_json, platt)
    metrics: dict[str, object] = {
            "features": features,
            "train": metrics_for(y[train_mask], pred[train_mask], weights[train_mask]),
            "validation": metrics_for(y[val_mask], pred[val_mask], weights[val_mask]),
            "test": metrics_for(y[test_mask], pred[test_mask], weights[test_mask]),
            "by_source_test": {},
            "weights": weights_json,
            "criticality_thresholds": criticality_thresholds(score[val_mask], weights[val_mask]),
            "score_percentiles": {
                "train": percentiles(score[train_mask]),
                "validation": percentiles(score[val_mask]),
                "test": percentiles(score[test_mask]),
            },
            "weighted_score_percentiles": {
                "train": weighted_percentiles(score[train_mask], weights[train_mask]),
                "validation": weighted_percentiles(score[val_mask], weights[val_mask]),
                "test": weighted_percentiles(score[test_mask], weights[test_mask]),
            },
            "validation_score_buckets": score_buckets(
                y[val_mask], pred[val_mask], score[val_mask], weights[val_mask]
            ),
    }
    by_source = metrics["by_source_test"]
    assert isinstance(by_source, dict)
    for source in sorted(set(sources)):
        mask = test_mask & (sources == source)
        if mask.sum() >= 2 and len(set(y[mask])) == 2:
            by_source[source] = metrics_for(y[mask], pred[mask], weights[mask])
    return metrics


def load_rows(path: Path, max_rows: int) -> list[dict[str, str]]:
    shards = [path] if path.is_file() else sorted(path.glob("criticality-*.csv"))
    if not shards:
        raise SystemExit(f"no CSV input found at {path}")

    rows: list[dict[str, str]] = []
    for shard in shards:
        with shard.open(newline="", encoding="utf8", errors="replace") as handle:
            reader = csv.DictReader(handle)
            for row in reader:
                if row.get("label_source") in ("", "none", None):
                    continue
                if row.get("bound_changed") not in ("0", "1"):
                    continue
                if "split" not in row or row.get("split") == "":
                    row["split"] = split_for(row.get("pid", ""), row.get("game_id", ""))
                rows.append(row)
                if max_rows and len(rows) >= max_rows:
                    return rows
    return rows


def build_matrix(rows: list[dict[str, str]], features: list[str]) -> np.ndarray:
    return np.array([[feature_value(row, feature) for feature in features] for row in rows], dtype=np.float64)


def feature_value(row: dict[str, str], feature: str) -> float:
    if feature == "side_to_move_black":
        return 1.0 if row.get("side_to_move") == "black" else 0.0
    if feature.startswith("piece_"):
        return 1.0 if row.get("piece_type") == feature.removeprefix("piece_") else 0.0

    value = parse_float(row.get(feature, ""))
    if feature == "history_score":
        return float(np.clip(value, -HISTORY_CLIP, HISTORY_CLIP) / HISTORY_CLIP)
    if feature in {"static_eval", "prev_static_eval", "static_eval_delta", "alpha", "beta"}:
        return float(np.clip(value, -SCORE_CLIP, SCORE_CLIP) / SCORE_CLIP)
    if feature in {"root_depth", "depth", "new_depth"}:
        return value / 16.0
    if feature == "ply":
        return value / 32.0
    if feature == "move_index":
        return value / 32.0
    if feature in {"base_reduction", "final_reduction"}:
        return value / 4.0
    return value


def parse_float(raw: str | None) -> float:
    if raw in (None, ""):
        return 0.0
    try:
        return float(raw)
    except ValueError:
        return 0.0


def inclusion_weights(rows: list[dict[str, str]], probe_permille: float) -> np.ndarray:
    probe_probability = max(probe_permille, 1e-9) / 1000.0
    counterfactual_weight = 1.0 / probe_probability
    return np.array(
        [counterfactual_weight if row.get("label_source") == "counterfactual_probe" else 1.0 for row in rows],
        dtype=np.float64,
    )


def fit_logreg(
    X: np.ndarray,
    y: np.ndarray,
    weights: np.ndarray,
    epochs: int,
    lr: float,
    l2: float,
) -> dict[str, np.ndarray]:
    mean = X.mean(axis=0)
    std = X.std(axis=0)
    std[std < 1e-9] = 1.0
    Xn = (X - mean) / std
    Xb = np.column_stack([np.ones(Xn.shape[0]), Xn])
    w = np.zeros(Xb.shape[1], dtype=np.float64)

    # Initialize intercept to the empirical log-odds.
    weights = weights / weights.mean()
    p = np.clip(np.average(y, weights=weights), 1e-6, 1 - 1e-6)
    w[0] = math.log(p / (1 - p))

    m = np.zeros_like(w)
    v = np.zeros_like(w)
    beta1, beta2, eps = 0.9, 0.999, 1e-8
    weight_sum = float(weights.sum())
    reg_mask = np.r_[0.0, np.ones(Xb.shape[1] - 1)]
    for epoch in range(1, epochs + 1):
        pred = sigmoid(Xb @ w)
        grad = (Xb.T @ ((pred - y) * weights)) / weight_sum + l2 * reg_mask * w
        m = beta1 * m + (1 - beta1) * grad
        v = beta2 * v + (1 - beta2) * (grad * grad)
        m_hat = m / (1 - beta1**epoch)
        v_hat = v / (1 - beta2**epoch)
        w -= lr * m_hat / (np.sqrt(v_hat) + eps)
    return {"mean": mean, "std": std, "w": w}


def predict_logreg(model: dict[str, np.ndarray], X: np.ndarray) -> np.ndarray:
    return sigmoid(decision_logreg(model, X))


def decision_logreg(model: dict[str, np.ndarray], X: np.ndarray) -> np.ndarray:
    Xn = (X - model["mean"]) / model["std"]
    Xb = np.column_stack([np.ones(Xn.shape[0]), Xn])
    return Xb @ model["w"]


def fit_platt(
    logits: np.ndarray,
    y: np.ndarray,
    weights: np.ndarray,
    epochs: int,
    lr: float,
    l2: float,
) -> dict[str, float]:
    x = np.column_stack([np.ones(len(logits)), logits])
    weights = weights / weights.mean()
    p = np.clip(np.average(y, weights=weights), 1e-6, 1 - 1e-6)
    w = np.array([math.log(p / (1 - p)), 1.0], dtype=np.float64)
    m = np.zeros_like(w)
    v = np.zeros_like(w)
    beta1, beta2, eps = 0.9, 0.999, 1e-8
    reg_mask = np.array([0.0, 1.0])
    weight_sum = float(weights.sum())
    for epoch in range(1, epochs + 1):
        pred = sigmoid(x @ w)
        grad = (x.T @ ((pred - y) * weights)) / weight_sum + l2 * reg_mask * w
        m = beta1 * m + (1 - beta1) * grad
        v = beta2 * v + (1 - beta2) * (grad * grad)
        m_hat = m / (1 - beta1**epoch)
        v_hat = v / (1 - beta2**epoch)
        w -= lr * m_hat / (np.sqrt(v_hat) + eps)
    return {"intercept": float(w[0]), "slope": float(w[1])}


def apply_platt(platt: dict[str, float], logits: np.ndarray) -> np.ndarray:
    return sigmoid(platt["intercept"] + platt["slope"] * logits)


def sigmoid(z: np.ndarray) -> np.ndarray:
    return 1.0 / (1.0 + np.exp(-np.clip(z, -40, 40)))


def metrics_for(y: np.ndarray, pred: np.ndarray, weights: np.ndarray) -> dict[str, float]:
    pred = np.clip(pred, 1e-12, 1 - 1e-12)
    weights = weights / weights.mean()
    return {
        "n": int(len(y)),
        "effective_n": float(weights.sum()),
        "positive_rate": float(np.average(y, weights=weights)),
        "auc": auc(y, pred, weights),
        "log_loss": float(np.average(-(y * np.log(pred) + (1 - y) * np.log(1 - pred)), weights=weights)),
        "brier": float(np.average((pred - y) ** 2, weights=weights)),
        "calibration_bins": calibration_bins(y, pred, weights),
    }


def calibration_bins(y: np.ndarray, pred: np.ndarray, weights: np.ndarray, bins: int = 10) -> list[dict[str, float]]:
    if len(y) == 0:
        return []
    order = np.argsort(pred)
    y = y[order]
    pred = pred[order]
    weights = weights[order]
    out = []
    for chunk in np.array_split(np.arange(len(y)), min(bins, len(y))):
        w = weights[chunk]
        out.append(
            {
                "n": int(len(chunk)),
                "weight": float(w.sum()),
                "avg_pred": float(np.average(pred[chunk], weights=w)),
                "empirical": float(np.average(y[chunk], weights=w)),
            }
        )
    return out


def percentiles(values: np.ndarray) -> dict[str, float]:
    if len(values) == 0:
        return {}
    qs = np.percentile(values, SCORE_PERCENTILES)
    return {percentile_key(p): float(q) for p, q in zip(SCORE_PERCENTILES, qs)}


def weighted_percentiles(values: np.ndarray, weights: np.ndarray) -> dict[str, float]:
    if len(values) == 0:
        return {}
    order = np.argsort(values)
    values = values[order]
    weights = weights[order]
    total = float(weights.sum())
    if total <= 0.0:
        return percentiles(values)
    cdf = np.cumsum(weights) / total
    return {
        percentile_key(p): float(values[min(int(np.searchsorted(cdf, p / 100.0, side="left")), len(values) - 1)])
        for p in SCORE_PERCENTILES
    }


def criticality_thresholds(validation_score: np.ndarray, validation_weights: np.ndarray) -> dict[str, float]:
    out = {f"validation_{key}": value for key, value in percentiles(validation_score).items()}
    out.update(
        {
            f"weighted_validation_{key}": value
            for key, value in weighted_percentiles(validation_score, validation_weights).items()
        }
    )
    return out


def score_buckets(
    y: np.ndarray,
    pred: np.ndarray,
    score: np.ndarray,
    weights: np.ndarray,
) -> list[dict[str, float | int | str | None]]:
    if len(score) == 0:
        return []
    thresholds = percentiles(score)
    edges = [
        ("bottom_to_p50", -math.inf, thresholds["p50"]),
        ("p50_to_p75", thresholds["p50"], thresholds["p75"]),
        ("p75_to_p90", thresholds["p75"], thresholds["p90"]),
        ("p90_to_p95", thresholds["p90"], thresholds["p95"]),
        ("p95_to_p97", thresholds["p95"], thresholds["p97"]),
        ("p97_to_p99", thresholds["p97"], thresholds["p99"]),
        ("p99_to_p99_5", thresholds["p99"], thresholds["p99_5"]),
        ("top_p99_5", thresholds["p99_5"], math.inf),
    ]
    out: list[dict[str, float | int | str | None]] = []
    for index, (name, lower, upper) in enumerate(edges):
        if index == 0:
            mask = score <= upper
        elif index == len(edges) - 1:
            mask = score > lower
        else:
            mask = (score > lower) & (score <= upper)
        if not np.any(mask):
            out.append(
                {
                    "bucket": name,
                    "n": 0,
                    "weight": 0.0,
                    "lower": finite_or_none(lower),
                    "upper": finite_or_none(upper),
                }
            )
            continue
        w = weights[mask]
        out.append(
            {
                "bucket": name,
                "n": int(mask.sum()),
                "weight": float(w.sum()),
                "lower": finite_or_none(lower),
                "upper": finite_or_none(upper),
                "avg_score": float(np.average(score[mask], weights=w)),
                "avg_pred": float(np.average(pred[mask], weights=w)),
                "empirical": float(np.average(y[mask], weights=w)),
            }
        )
    return out


def finite_or_none(value: float) -> float | None:
    return float(value) if math.isfinite(value) else None


def percentile_key(percentile: float) -> str:
    return f"p{percentile:g}".replace(".", "_")


def auc(y: np.ndarray, score: np.ndarray, weights: np.ndarray) -> float:
    pos_total = float(weights[y == 1].sum())
    neg_total = float(weights[y == 0].sum())
    if pos_total == 0.0 or neg_total == 0.0:
        return float("nan")
    order = np.argsort(score, kind="mergesort")
    sorted_score = score[order]
    sorted_y = y[order]
    sorted_w = weights[order]
    i = 0
    neg_below = 0.0
    numerator = 0.0
    while i < len(score):
        j = i + 1
        while j < len(score) and sorted_score[j] == sorted_score[i]:
            j += 1
        group_y = sorted_y[i:j]
        group_w = sorted_w[i:j]
        pos_w = float(group_w[group_y == 1].sum())
        neg_w = float(group_w[group_y == 0].sum())
        numerator += pos_w * neg_below + 0.5 * pos_w * neg_w
        neg_below += neg_w
        i = j
    return float(numerator / (pos_total * neg_total))


def model_to_weights(model: dict[str, np.ndarray], features: list[str]) -> dict[str, object]:
    # Convert standardized weights to raw-feature weights for easier Rust export later.
    w = model["w"]
    raw_weights = w[1:] / model["std"]
    raw_intercept = w[0] - np.sum(w[1:] * model["mean"] / model["std"])
    return {
        "intercept": float(raw_intercept),
        "features": {name: float(weight) for name, weight in zip(features, raw_weights)},
    }


def apply_platt_to_raw_weights(weights_json: dict[str, object], platt: dict[str, float]) -> dict[str, object]:
    slope = platt["slope"]
    features = weights_json["features"]
    assert isinstance(features, dict)
    return {
        "intercept": float(platt["intercept"] + slope * float(weights_json["intercept"])),
        "features": {name: float(slope * value) for name, value in features.items()},
    }


def platt_to_json(platt: dict[str, float]) -> dict[str, float]:
    return {"intercept": float(platt["intercept"]), "slope": float(platt["slope"])}


def print_counts(name: str, rows: list[dict[str, str]]) -> None:
    counts: dict[str, int] = {}
    for row in rows:
        key = row.get(name, "")
        counts[key] = counts.get(key, 0) + 1
    print(name, dict(sorted(counts.items())))


def print_model_summary(name: str, metrics: dict[str, object]) -> None:
    print(f"model {name}")
    for split in ["train", "validation", "test"]:
        m = metrics[split]
        print(
            f"  {split} n={m['n']} pos={m['positive_rate']:.4f} "
            f"auc={m['auc']:.6f} logloss={m['log_loss']:.6f} brier={m['brier']:.6f}"
        )
    for source, m in metrics["by_source_test"].items():
        print(
            f"  test/{source} n={m['n']} pos={m['positive_rate']:.4f} "
            f"auc={m['auc']:.6f} logloss={m['log_loss']:.6f} brier={m['brier']:.6f}"
        )


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
