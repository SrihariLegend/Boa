#!/usr/bin/env python3
"""Train/evaluate a lightweight logistic criticality model.

The script is dependency-light for CSV input: only numpy is required. It can
also read Parquet files/dataset directories when pyarrow is installed.
"""

from __future__ import annotations

import argparse
import csv
import gzip
import hashlib
import json
import math
from pathlib import Path


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

FEATURES_NULL_MOVE_FULL = [
    "root_depth",
    "ply",
    "depth",
    "null_reduction",
    "null_depth",
    "static_eval",
    "has_prev_static_eval",
    "prev_static_eval",
    "static_eval_delta",
    "alpha",
    "beta",
    "static_beta_margin",
    "is_cut_node",
    "improving",
    "side_to_move_black",
]

FEATURES_NULL_MOVE_MARGIN_ONLY = ["static_beta_margin"]

FEATURES_FUTILITY_FULL = [
    "root_depth",
    "ply",
    "depth",
    "move_index",
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
    "side_to_move_black",
    "piece_p",
    "piece_n",
    "piece_b",
    "piece_r",
    "piece_q",
    "piece_k",
]

FEATURES_FUTILITY_MARGIN_ONLY = ["static_alpha_margin"]

FEATURES_UNIFIED_KIND_ONLY = ["kind_lmr", "kind_futility", "kind_rfp"]

FEATURES_UNIFIED_FULL = [
    "kind_lmr",
    "kind_futility",
    "kind_rfp",
    "depth",
    "ply",
    "node_type",
    "improving",
    "static_eval",
    "alpha",
    "beta",
    "volatility",
    "king_danger",
    "phase",
    "move_index",
    "history_score",
    "planned_reduction",
    "planned_margin",
    "gap",
    "has_move_index",
    "has_history",
    "has_reduction",
    "has_margin",
    "has_gap",
    "node_count_log",
]

FEATURES_FUTILITY_PRODUCTION = [
    feature
    for feature in FEATURES_UNIFIED_FULL
    if not feature.startswith("kind_") and feature != "node_count_log"
]

SCORE_CLIP = 2000.0
HISTORY_CLIP = 16384.0
SCORE_PERCENTILES = [50.0, 75.0, 90.0, 95.0, 97.0, 98.0, 99.0, 99.5]
TOP_FRACTIONS = [0.001, 0.005, 0.01, 0.05]
MIN_STABLE_POSITIVES = 20


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "data",
        nargs="+",
        help="One or more labeled CSV/Parquet files, raw CSV dirs, or Parquet dataset dirs",
    )
    parser.add_argument(
        "--decision",
        choices=["lmr", "null_move", "futility", "rfp", "unified"],
        default="unified",
        help="Decision rows to train/evaluate. Missing decision_kind is treated as lmr.",
    )
    parser.add_argument("--epochs", type=int, default=250)
    parser.add_argument("--lr", type=float, default=0.03)
    parser.add_argument("--l2", type=float, default=1e-4)
    parser.add_argument(
        "--target",
        choices=["bound_changed", "regret_cp", "score_delta"],
        default="bound_changed",
        help=(
            "Training target. bound_changed uses logistic classification; regret_cp/score_delta "
            "use weighted linear regression, useful for ultra-rare FFP positives."
        ),
    )
    parser.add_argument(
        "--regression-transform",
        choices=["log1p", "raw"],
        default="log1p",
        help="Target transform for regret_cp/score_delta regression.",
    )
    parser.add_argument(
        "--target-clip-cp",
        type=float,
        default=2000.0,
        help="Clip regression targets to this many cp before fitting/evaluation; 0 disables clipping.",
    )
    parser.add_argument(
        "--probe-permille",
        type=float,
        default=5.0,
        help="Counterfactual probe rate used during collection; used for inverse-probability weights",
    )
    parser.add_argument("--max-rows", type=int, default=0, help="Optional deterministic cap on labeled rows")
    parser.add_argument(
        "--split-mode",
        choices=["input", "stratified"],
        default="input",
        help=(
            "Split assignment. 'input' preserves/derives the normal hash split; "
            "'stratified' deterministically re-splits within decision_kind x label, useful for rare-positive diagnostics."
        ),
    )
    parser.add_argument(
        "--label-source",
        action="append",
        choices=["observed_research", "counterfactual_probe"],
        help=(
            "Restrict training/evaluation to one label source. Repeat to allow multiple sources. "
            "Use --label-source counterfactual_probe for shadow-only counterfactual data."
        ),
    )
    parser.add_argument("--out", help="Optional JSON output for model weights/metrics")
    parser.add_argument(
        "--summary-only",
        action="store_true",
        help="Load rows and print bounded aggregate counts without fitting a model.",
    )
    args = parser.parse_args()

    model_specs = model_specs_for_decision(args.decision)
    rows = load_rows([Path(path) for path in args.data], args.max_rows, set(args.label_source or []), args.decision)
    if not rows:
        raise SystemExit(f"no labeled {args.decision} rows found")
    if args.split_mode == "stratified":
        apply_stratified_splits(rows)
    validate_required_columns(rows, model_specs, args.decision)

    print(f"loaded_labeled_rows {len(rows)}")
    print_counts("decision_kind", rows)
    print_counts("split", rows)
    print_split_kind_label_counts(rows)
    if any("label_source" in row for row in rows):
        print_counts("label_source", rows)
    print_counts("bound_changed", rows)

    if args.summary_only:
        return

    global np
    try:
        import numpy as np  # type: ignore[import-not-found]
    except ImportError as exc:
        raise SystemExit("numpy is required for training: python3 -m pip install numpy") from exc

    y_class = np.array([int(row["bound_changed"]) for row in rows], dtype=np.float64)
    weights = inclusion_weights(rows, args.probe_permille)
    splits = np.array([row.get("split") or split_for_row(row) for row in rows])
    sources = np.array([row.get("label_source", "") for row in rows])
    kinds = np.array([row.get("decision_kind", "") for row in rows])

    train_mask = splits == "train"
    val_mask = splits == "validation"
    test_mask = splits == "test"
    if train_mask.sum() == 0 or val_mask.sum() == 0 or test_mask.sum() == 0:
        raise SystemExit(
            f"empty split: train={train_mask.sum()} validation={val_mask.sum()} test={test_mask.sum()}"
        )

    if args.target != "bound_changed":
        run_regression_training(
            rows,
            model_specs,
            args.target,
            args.regression_transform,
            args.target_clip_cp,
            weights,
            kinds,
            train_mask,
            val_mask,
            test_mask,
            args.epochs,
            args.lr,
            args.l2,
            args.out,
        )
        return

    y = y_class
    class_count = len(set(y.tolist()))
    if class_count < 2:
        raise SystemExit(
            "need both bound_changed classes to train/evaluate; "
            f"found only bound_changed={int(y[0])} in {len(rows)} rows"
        )

    results = {}
    for name, features in model_specs:
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
            kinds,
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
            kinds,
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

    baseline_name = model_specs[0][0]
    if baseline_name != "full" and "full" in results:
        delta_auc = results["full"]["test"]["auc"] - results[baseline_name]["test"]["auc"]
        delta_auc_platt = (
            results["full"]["calibrated"]["test"]["auc"]
            - results[baseline_name]["calibrated"]["test"]["auc"]
        )
        print(f"test_auc_delta_full_minus_{baseline_name} {delta_auc:.6f}")
        print(f"test_auc_delta_full_platt_minus_{baseline_name}_platt {delta_auc_platt:.6f}")

    if args.out:
        out_path = Path(args.out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(json.dumps(results, indent=2), encoding="utf8")
        print(f"wrote {out_path}")


def run_regression_training(
    rows: list[dict[str, str]],
    model_specs: list[tuple[str, list[str]]],
    target_name: str,
    transform: str,
    target_clip_cp: float,
    weights: np.ndarray,
    kinds: np.ndarray,
    train_mask: np.ndarray,
    val_mask: np.ndarray,
    test_mask: np.ndarray,
    epochs: int,
    lr: float,
    l2: float,
    out: str | None,
) -> None:
    y_raw = np.array([regression_target(row, target_name) for row in rows], dtype=np.float64)
    if target_clip_cp > 0.0:
        y_raw = np.minimum(y_raw, target_clip_cp)
    y_train = transform_regression_target(y_raw, transform)
    print(
        f"regression_target {target_name} transform={transform} clip_cp={target_clip_cp:g} "
        f"nonzero={int(np.count_nonzero(y_raw))} mean={float(np.average(y_raw, weights=weights)):.6f}"
    )
    results = {}
    for name, features in model_specs:
        X = build_matrix(rows, features)
        model = fit_linear_regression(
            X[train_mask],
            y_train[train_mask],
            weights[train_mask],
            epochs=epochs,
            lr=lr,
            l2=l2,
        )
        pred_transformed = predict_linear(model, X)
        pred = inverse_regression_target(pred_transformed, transform)
        metrics = build_regression_result_metrics(
            features,
            model,
            y_raw,
            pred,
            weights,
            kinds,
            train_mask,
            val_mask,
            test_mask,
            target_name,
            transform,
            target_clip_cp,
        )
        results[name] = metrics
        print_regression_model_summary(name, metrics)

    baseline_name = model_specs[0][0]
    for candidate_name in ["production", "full"]:
        if baseline_name != candidate_name and candidate_name in results:
            delta_corr = results[candidate_name]["test"]["pearson"] - results[baseline_name]["test"]["pearson"]
            print(f"test_pearson_delta_{candidate_name}_minus_{baseline_name} {delta_corr:.6f}")

    if out:
        out_path = Path(out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(json.dumps(results, indent=2), encoding="utf8")
        print(f"wrote {out_path}")


def regression_target(row: dict[str, str], target_name: str) -> float:
    if target_name == "score_delta":
        return max(0.0, parse_float(row.get("score_delta", "")))
    if row.get("regret_cp") not in (None, ""):
        return max(0.0, parse_float(row.get("regret_cp", "")))
    kind = normalize_decision(row.get("decision_kind", ""))
    full = parse_float(row.get("full_score", ""))
    pruned = parse_float(row.get("pruned_score", ""))
    if kind == "rfp":
        return max(0.0, pruned - full)
    return max(0.0, full - pruned)


def transform_regression_target(y: np.ndarray, transform: str) -> np.ndarray:
    if transform == "log1p":
        return np.log1p(np.maximum(0.0, y))
    return y.copy()


def inverse_regression_target(y: np.ndarray, transform: str) -> np.ndarray:
    if transform == "log1p":
        return np.maximum(0.0, np.expm1(np.clip(y, 0.0, 12.0)))
    return np.maximum(0.0, y)


def build_result_metrics(
    features: list[str],
    model: dict[str, np.ndarray],
    y: np.ndarray,
    weights: np.ndarray,
    sources: np.ndarray,
    kinds: np.ndarray,
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
            "by_kind": {},
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
        if source == "":
            continue
        mask = test_mask & (sources == source)
        if mask.sum() >= 2 and len(set(y[mask])) == 2:
            by_source[source] = metrics_for(y[mask], pred[mask], weights[mask])
    by_kind = metrics["by_kind"]
    assert isinstance(by_kind, dict)
    split_masks = {"train": train_mask, "validation": val_mask, "test": test_mask}
    for kind in sorted(set(kinds)):
        kind_metrics = {}
        for split_name, split_mask in split_masks.items():
            mask = split_mask & (kinds == kind)
            if mask.sum() > 0:
                kind_metrics[split_name] = metrics_for(y[mask], pred[mask], weights[mask])
        if kind_metrics:
            by_kind[kind] = kind_metrics
    return metrics


def model_specs_for_decision(decision: str) -> list[tuple[str, list[str]]]:
    if decision == "null_move":
        return [
            ("static_margin_only", FEATURES_NULL_MOVE_MARGIN_ONLY),
            ("full", FEATURES_NULL_MOVE_FULL),
        ]
    if decision == "futility":
        return [
            ("margin_only", ["gap", "planned_margin"]),
            ("production", FEATURES_FUTILITY_PRODUCTION),
            ("full", [feature for feature in FEATURES_UNIFIED_FULL if not feature.startswith("kind_")]),
        ]
    if decision == "rfp":
        return [
            ("margin_only", ["gap", "planned_margin"]),
            ("full", [feature for feature in FEATURES_UNIFIED_FULL if not feature.startswith("kind_")]),
        ]
    if decision == "unified":
        return [
            ("kind_only", FEATURES_UNIFIED_KIND_ONLY),
            ("full", FEATURES_UNIFIED_FULL),
        ]
    return [
        ("history_only", FEATURES_HISTORY_ONLY),
        ("full", FEATURES_FULL),
    ]


def load_rows(
    paths: list[Path],
    max_rows: int,
    label_sources: set[str],
    decision: str,
) -> list[dict[str, str]]:
    if not paths:
        return []
    if any(is_parquet_input(path) for path in paths):
        if len(paths) != 1:
            raise SystemExit("multiple input paths are currently supported for CSV/CSV.GZ only")
        return load_parquet_rows(paths[0], max_rows, label_sources, decision)

    shards: list[Path] = []
    input_diagnostics: list[str] = []
    for path in paths:
        path_shards = csv_shards_for_path(path)
        shards.extend(path_shards)
        if path_shards:
            input_diagnostics.append(f"{path}: {len(path_shards)} CSV shard(s)")
        elif not path.exists():
            input_diagnostics.append(f"{path}: missing")
        elif path.is_dir():
            parquet_count = len(list(path.glob("*.parquet")))
            input_diagnostics.append(
                f"{path}: directory contains no criticality-*.csv/csv.gz shards"
                + (f" and {parquet_count} top-level parquet file(s)" if parquet_count else "")
            )
        else:
            input_diagnostics.append(f"{path}: not a readable CSV shard directory")
    if not shards:
        details = "\n  ".join(input_diagnostics) if input_diagnostics else "<no inputs>"
        raise SystemExit(
            "no CSV input found; expected files named criticality-*.csv or "
            "criticality-*.csv.gz in each input directory, or pass a single "
            f"Parquet dataset directory. Inputs checked:\n  {details}"
        )

    rows: list[dict[str, str]] = []
    row_index = 0
    for shard in shards:
        with open_csv_text(shard) as handle:
            reader = csv.DictReader(handle)
            for row in reader:
                row_decision = normalize_decision(row.get("decision_kind") or "lmr")
                if decision != "unified" and row_decision != decision:
                    continue
                row["decision_kind"] = row_decision
                if "label_source" in row and row.get("label_source") in ("", "none", None):
                    row["label_source"] = "counterfactual_probe"
                if label_sources and row.get("label_source") not in label_sources:
                    continue
                if row.get("bound_changed") not in ("0", "1"):
                    continue
                if "split" not in row or row.get("split") == "":
                    row["split"] = split_for_row(row, row_index)
                row["row_index"] = str(row_index)
                add_unified_compat_columns(row)
                rows.append(row)
                row_index += 1
                if max_rows and len(rows) >= max_rows:
                    return rows
    return rows


def csv_shards_for_path(path: Path) -> list[Path]:
    if path.is_file():
        return [path]
    return sorted(path.glob("criticality-*.csv")) + sorted(path.glob("criticality-*.csv.gz"))


def apply_stratified_splits(rows: list[dict[str, str]]) -> None:
    groups: dict[tuple[str, str], int] = {}
    for row in rows:
        key = (row.get("decision_kind", ""), row.get("bound_changed", ""))
        index = groups.get(key, 0)
        groups[key] = index + 1
        bucket = index % 10
        if bucket == 0:
            row["split"] = "test"
        elif bucket == 1:
            row["split"] = "validation"
        else:
            row["split"] = "train"


def open_csv_text(path: Path):
    if path.name.endswith(".gz"):
        return gzip.open(path, "rt", newline="", encoding="utf8", errors="replace")
    return path.open(newline="", encoding="utf8", errors="replace")


def normalize_decision(value: object) -> str:
    text = str(value).strip().lower()
    if text == "lmr":
        return "lmr"
    if text in {"futility", "ffp"}:
        return "futility"
    if text == "rfp":
        return "rfp"
    return text


def add_unified_compat_columns(row: dict[str, str]) -> None:
    kind = normalize_decision(row.get("decision_kind", ""))
    row["kind_lmr"] = "1" if kind == "lmr" else "0"
    row["kind_futility"] = "1" if kind == "futility" else "0"
    row["kind_rfp"] = "1" if kind == "rfp" else "0"
    # Legacy-column aliases used by older model specs.
    row.setdefault("final_reduction", row.get("planned_reduction", "0"))
    row.setdefault("base_reduction", row.get("planned_reduction", "0"))
    row.setdefault("new_depth", str(max(0, int(parse_float(row.get("depth", "0"))) - 1)))
    row.setdefault("futility_margin", row.get("planned_margin", "0"))
    row.setdefault("static_alpha_margin", row.get("gap", "0"))


def is_parquet_input(path: Path) -> bool:
    if path.is_file() and path.suffix == ".parquet":
        return True
    return path.is_dir() and any(path.glob("*.parquet"))


def load_parquet_rows(
    path: Path,
    max_rows: int,
    label_sources: set[str],
    decision: str,
) -> list[dict[str, str]]:
    try:
        import pyarrow.dataset as ds
    except ImportError as exc:
        raise SystemExit("pyarrow is required for Parquet input: python3 -m pip install pyarrow") from exc

    rows: list[dict[str, str]] = []
    dataset = ds.dataset(path, format="parquet")
    for batch in dataset.to_batches(batch_size=65_536):
        for row in batch.to_pylist():
            row_decision = normalize_decision(row.get("decision_kind") or "lmr")
            if decision != "unified" and row_decision != decision:
                continue
            row["decision_kind"] = row_decision
            if "label_source" in row and row.get("label_source") in ("", "none", None):
                row["label_source"] = "counterfactual_probe"
            if label_sources and row.get("label_source") not in label_sources:
                continue
            if str(row.get("bound_changed")) not in ("0", "1"):
                continue
            if "split" not in row or row.get("split") == "":
                row["split"] = split_for_row(row, len(rows))
            add_unified_compat_columns(row)
            rows.append(row)
            if max_rows and len(rows) >= max_rows:
                return rows
    return rows


def validate_required_columns(
    rows: list[dict[str, str]],
    model_specs: list[tuple[str, list[str]]],
    decision: str,
) -> None:
    required = set[str]()
    for _, features in model_specs:
        for feature in features:
            required.update(raw_columns_for_feature(feature))

    missing = sorted(column for column in required if any(column not in row for row in rows))
    if missing:
        raise SystemExit(
            f"missing required columns for {decision}: {', '.join(missing)}"
        )


def raw_columns_for_feature(feature: str) -> set[str]:
    if feature == "side_to_move_black":
        return {"side_to_move"}
    if feature.startswith("piece_"):
        return {"piece_type"}
    if feature == "node_count_log":
        return {"node_count"}
    return {feature}


def build_matrix(rows: list[dict[str, str]], features: list[str]) -> np.ndarray:
    return np.array([[feature_value(row, feature) for feature in features] for row in rows], dtype=np.float64)


def feature_value(row: dict[str, str], feature: str) -> float:
    if feature == "side_to_move_black":
        return 1.0 if row.get("side_to_move") == "black" else 0.0
    if feature.startswith("piece_"):
        return 1.0 if row.get("piece_type") == feature.removeprefix("piece_") else 0.0
    if feature == "node_count_log":
        return math.log1p(max(0.0, parse_float(row.get("node_count", "")))) / 10.0

    value = parse_float(row.get(feature, ""))
    if feature == "history_score":
        return float(np.clip(value, -HISTORY_CLIP, HISTORY_CLIP) / HISTORY_CLIP)
    if feature in {
        "static_eval",
        "prev_static_eval",
        "static_eval_delta",
        "alpha",
        "beta",
        "static_beta_margin",
        "futility_margin",
        "static_alpha_margin",
        "volatility",
        "king_danger",
        "planned_margin",
        "gap",
    }:
        return float(np.clip(value, -SCORE_CLIP, SCORE_CLIP) / SCORE_CLIP)
    if feature in {"root_depth", "depth", "new_depth", "null_depth"}:
        return value / 16.0
    if feature == "ply":
        return value / 32.0
    if feature == "move_index":
        return value / 32.0
    if feature in {"base_reduction", "final_reduction", "null_reduction", "planned_reduction"}:
        return value / 4.0
    if feature == "node_type":
        return value / 2.0
    return value


def parse_float(raw: str | None) -> float:
    if raw in (None, ""):
        return 0.0
    try:
        return float(raw)
    except ValueError:
        return 0.0


def inclusion_weights(rows: list[dict[str, str]], probe_permille: float) -> np.ndarray:
    fallback_probability = max(probe_permille, 1e-9) / 1000.0
    out = []
    for row in rows:
        if row.get("label_source", "counterfactual_probe") != "counterfactual_probe":
            out.append(1.0)
            continue
        row_permille = parse_float(row.get("sample_permille", ""))
        probability = (row_permille / 1000.0) if row_permille > 0 else fallback_probability
        out.append(1.0 / max(probability, 1e-9))
    return np.array(out, dtype=np.float64)


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


def fit_linear_regression(
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
    weights = weights / weights.mean()
    w[0] = float(np.average(y, weights=weights))

    m = np.zeros_like(w)
    v = np.zeros_like(w)
    beta1, beta2, eps = 0.9, 0.999, 1e-8
    weight_sum = float(weights.sum())
    reg_mask = np.r_[0.0, np.ones(Xb.shape[1] - 1)]
    for epoch in range(1, epochs + 1):
        pred = Xb @ w
        grad = (Xb.T @ ((pred - y) * weights)) / weight_sum + l2 * reg_mask * w
        m = beta1 * m + (1 - beta1) * grad
        v = beta2 * v + (1 - beta2) * (grad * grad)
        m_hat = m / (1 - beta1**epoch)
        v_hat = v / (1 - beta2**epoch)
        w -= lr * m_hat / (np.sqrt(v_hat) + eps)
    return {"mean": mean, "std": std, "w": w}


def predict_linear(model: dict[str, np.ndarray], X: np.ndarray) -> np.ndarray:
    Xn = (X - model["mean"]) / model["std"]
    Xb = np.column_stack([np.ones(Xn.shape[0]), Xn])
    return Xb @ model["w"]


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
    bins = calibration_bins(y, pred, weights)
    positives = int((y == 1).sum())
    negatives = int((y == 0).sum())
    return {
        "n": int(len(y)),
        "positives": positives,
        "negatives": negatives,
        "weighted_positives": float(weights[y == 1].sum()),
        "weighted_negatives": float(weights[y == 0].sum()),
        "effective_n": float(weights.sum()),
        "positive_rate": float(np.average(y, weights=weights)),
        "auc": auc(y, pred, weights),
        "average_precision": average_precision(y, pred, weights),
        "log_loss": float(np.average(-(y * np.log(pred) + (1 - y) * np.log(1 - pred)), weights=weights)),
        "brier": float(np.average((pred - y) ** 2, weights=weights)),
        "ece_10": calibration_error(bins),
        "low_positive_warning": positives < MIN_STABLE_POSITIVES,
        "top_capture": top_capture(y, pred, weights),
        "calibration_bins": bins,
    }


def build_regression_result_metrics(
    features: list[str],
    model: dict[str, np.ndarray],
    y: np.ndarray,
    pred: np.ndarray,
    weights: np.ndarray,
    kinds: np.ndarray,
    train_mask: np.ndarray,
    val_mask: np.ndarray,
    test_mask: np.ndarray,
    target_name: str,
    transform: str,
    target_clip_cp: float,
) -> dict[str, object]:
    metrics: dict[str, object] = {
        "target": target_name,
        "transform": transform,
        "target_clip_cp": target_clip_cp,
        "features": features,
        "feature_indices": {name: index for index, name in enumerate(features)},
        "train": regression_metrics_for(y[train_mask], pred[train_mask], weights[train_mask]),
        "validation": regression_metrics_for(y[val_mask], pred[val_mask], weights[val_mask]),
        "test": regression_metrics_for(y[test_mask], pred[test_mask], weights[test_mask]),
        "by_kind": {},
        "weights": model_to_weights(model, features),
        "standardized_model": model_to_standardized(model, features),
        "prediction_percentiles": {
            "train": weighted_percentiles(pred[train_mask], weights[train_mask]),
            "validation": weighted_percentiles(pred[val_mask], weights[val_mask]),
            "test": weighted_percentiles(pred[test_mask], weights[test_mask]),
        },
    }
    by_kind = metrics["by_kind"]
    assert isinstance(by_kind, dict)
    split_masks = {"train": train_mask, "validation": val_mask, "test": test_mask}
    for kind in sorted(set(kinds)):
        kind_metrics = {}
        for split_name, split_mask in split_masks.items():
            mask = split_mask & (kinds == kind)
            if mask.sum() > 0:
                kind_metrics[split_name] = regression_metrics_for(y[mask], pred[mask], weights[mask])
        if kind_metrics:
            by_kind[kind] = kind_metrics
    return metrics


def regression_metrics_for(y: np.ndarray, pred: np.ndarray, weights: np.ndarray) -> dict[str, object]:
    pred = np.maximum(0.0, pred)
    weights = weights / weights.mean()
    err = pred - y
    nonzero = int(np.count_nonzero(y > 0.0))
    mse = float(np.average(err * err, weights=weights))
    mae = float(np.average(np.abs(err), weights=weights))
    mean_y = float(np.average(y, weights=weights))
    mean_pred = float(np.average(pred, weights=weights))
    return {
        "n": int(len(y)),
        "nonzero": nonzero,
        "nonzero_rate": float(nonzero / len(y)) if len(y) else 0.0,
        "weighted_nonzero": float(weights[y > 0.0].sum()),
        "mean_target": mean_y,
        "mean_pred": mean_pred,
        "rmse": math.sqrt(mse),
        "mae": mae,
        "pearson": weighted_corr(y, pred, weights),
        "rank_auc_nonzero": auc((y > 0.0).astype(np.float64), pred, weights),
        "average_precision_nonzero": average_precision((y > 0.0).astype(np.float64), pred, weights),
        "target_percentiles": weighted_percentiles(y, weights),
        "pred_percentiles": weighted_percentiles(pred, weights),
        "top_target_capture": top_target_capture(y, pred, weights),
        "low_nonzero_warning": nonzero < MIN_STABLE_POSITIVES,
    }


def weighted_corr(x: np.ndarray, y: np.ndarray, weights: np.ndarray) -> float:
    if len(x) == 0:
        return float("nan")
    x_mean = float(np.average(x, weights=weights))
    y_mean = float(np.average(y, weights=weights))
    x_centered = x - x_mean
    y_centered = y - y_mean
    cov = float(np.average(x_centered * y_centered, weights=weights))
    x_var = float(np.average(x_centered * x_centered, weights=weights))
    y_var = float(np.average(y_centered * y_centered, weights=weights))
    if x_var <= 0.0 or y_var <= 0.0:
        return float("nan")
    return cov / math.sqrt(x_var * y_var)


def top_target_capture(y: np.ndarray, score: np.ndarray, weights: np.ndarray) -> list[dict[str, float | int]]:
    if len(y) == 0:
        return []
    order = np.argsort(-score, kind="mergesort")
    sorted_score = score[order]
    sorted_y = y[order]
    sorted_w = weights[order]
    total_target = float(np.sum(y * weights))
    out: list[dict[str, float | int]] = []
    for fraction in TOP_FRACTIONS:
        target_n = max(1, int(math.ceil(len(y) * fraction)))
        cutoff = sorted_score[target_n - 1]
        n = int(np.searchsorted(-sorted_score, -cutoff, side="right"))
        y_top = sorted_y[:n]
        w_top = sorted_w[:n]
        captured = float(np.sum(y_top * w_top))
        out.append(
            {
                "fraction": fraction,
                "n": n,
                "weighted_target": captured,
                "target_recall": captured / total_target if total_target > 0.0 else float("nan"),
                "avg_target": captured / float(w_top.sum()) if w_top.sum() > 0.0 else 0.0,
            }
        )
    return out


def calibration_error(bins: list[dict[str, float]]) -> float:
    total = sum(bin["weight"] for bin in bins)
    if total <= 0.0:
        return float("nan")
    return float(sum(bin["weight"] * abs(bin["avg_pred"] - bin["empirical"]) for bin in bins) / total)


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


def average_precision(y: np.ndarray, score: np.ndarray, weights: np.ndarray) -> float:
    pos_total = float(weights[y == 1].sum())
    if pos_total == 0.0:
        return float("nan")
    order = np.argsort(-score, kind="mergesort")
    sorted_score = score[order]
    sorted_y = y[order]
    sorted_w = weights[order]
    seen_weight = 0.0
    seen_pos = 0.0
    area = 0.0
    i = 0
    while i < len(score):
        j = i + 1
        while j < len(score) and sorted_score[j] == sorted_score[i]:
            j += 1
        group_y = sorted_y[i:j]
        group_w = sorted_w[i:j]
        group_weight = float(group_w.sum())
        group_pos = float(group_w[group_y == 1].sum())
        seen_weight += group_weight
        seen_pos += group_pos
        precision_after_group = seen_pos / max(seen_weight, 1e-12)
        area += group_pos * precision_after_group
        i = j
    return float(area / pos_total)


def top_capture(y: np.ndarray, score: np.ndarray, weights: np.ndarray) -> list[dict[str, float | int]]:
    if len(y) == 0:
        return []
    order = np.argsort(-score, kind="mergesort")
    sorted_score = score[order]
    sorted_y = y[order]
    sorted_w = weights[order]
    pos_total = float(weights[y == 1].sum())
    base_rate = pos_total / float(weights.sum()) if weights.sum() > 0.0 else 0.0
    out: list[dict[str, float | int]] = []
    for fraction in TOP_FRACTIONS:
        target_n = max(1, int(math.ceil(len(y) * fraction)))
        cutoff = sorted_score[target_n - 1]
        n = int(np.searchsorted(-sorted_score, -cutoff, side="right"))
        y_top = sorted_y[:n]
        w_top = sorted_w[:n]
        top_weight = float(w_top.sum())
        top_pos = float(w_top[y_top == 1].sum())
        precision = top_pos / top_weight if top_weight > 0.0 else 0.0
        recall = top_pos / pos_total if pos_total > 0.0 else float("nan")
        lift = precision / base_rate if base_rate > 0.0 else float("nan")
        out.append(
            {
                "fraction": fraction,
                "n": n,
                "weighted_positives": top_pos,
                "precision": precision,
                "recall": recall,
                "lift": lift,
            }
        )
    return out


def model_to_weights(model: dict[str, np.ndarray], features: list[str]) -> dict[str, object]:
    # Convert standardized weights to raw-feature weights for easier Rust export later.
    w = model["w"]
    raw_weights = w[1:] / model["std"]
    raw_intercept = w[0] - np.sum(w[1:] * model["mean"] / model["std"])
    return {
        "intercept": float(raw_intercept),
        "features": {name: float(weight) for name, weight in zip(features, raw_weights)},
    }



def model_to_standardized(model: dict[str, np.ndarray], features: list[str]) -> dict[str, object]:
    """Serialize the fitted standardized linear/logistic model exactly as trained."""
    w = model["w"]
    return {
        "intercept": float(w[0]),
        "coefficients": [float(value) for value in w[1:]],
        "features": list(features),
        "feature_indices": {name: index for index, name in enumerate(features)},
        "mean": [float(value) for value in model["mean"]],
        "std": [float(value) for value in model["std"]],
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


def print_split_kind_label_counts(rows: list[dict[str, str]]) -> None:
    summary: dict[str, dict[str, dict[str, int | float]]] = {}
    for row in rows:
        split = row.get("split", "")
        kind = row.get("decision_kind", "")
        label = 1 if row.get("bound_changed") == "1" else 0
        split_summary = summary.setdefault(split, {})
        entry = split_summary.setdefault(kind, {"n": 0, "positives": 0, "negatives": 0, "positive_rate": 0.0})
        entry["n"] = int(entry["n"]) + 1
        if label:
            entry["positives"] = int(entry["positives"]) + 1
        else:
            entry["negatives"] = int(entry["negatives"]) + 1
    for split_summary in summary.values():
        for entry in split_summary.values():
            n = int(entry["n"])
            entry["positive_rate"] = float(entry["positives"]) / n if n else 0.0
    print("split_kind_labels")
    for split, split_summary in sorted(summary.items()):
        for kind, entry in sorted(split_summary.items()):
            print(
                f"  {split}/{kind} n={entry['n']} positives={entry['positives']} "
                f"negatives={entry['negatives']} pos={entry['positive_rate']:.6g}"
            )


def print_model_summary(name: str, metrics: dict[str, object]) -> None:
    print(f"model {name}")
    for split in ["train", "validation", "test"]:
        m = metrics[split]
        warning = " low_pos" if m.get("low_positive_warning") else ""
        print(
            f"  {split} n={m['n']} positives={m['positives']} pos={m['positive_rate']:.4f} "
            f"auc={m['auc']:.6f} ap={m['average_precision']:.6f} logloss={m['log_loss']:.6f} "
            f"brier={m['brier']:.6f} ece10={m['ece_10']:.6f}"
            f"{warning}"
        )
    by_kind = metrics.get("by_kind", {})
    if isinstance(by_kind, dict):
        for kind, split_metrics in by_kind.items():
            if not isinstance(split_metrics, dict):
                continue
            for split in ["train", "validation", "test"]:
                m = split_metrics.get(split)
                if not isinstance(m, dict):
                    continue
                warning = " low_pos" if m.get("low_positive_warning") else ""
                print(
                    f"  {split}/{kind} n={m['n']} positives={m['positives']} "
                    f"pos={m['positive_rate']:.4f} auc={m['auc']:.6f} "
                    f"ap={m['average_precision']:.6f} logloss={m['log_loss']:.6f}"
                    f"{warning}"
                )
    for source, m in metrics["by_source_test"].items():
        print(
            f"  test/{source} n={m['n']} positives={m['positives']} pos={m['positive_rate']:.4f} "
            f"auc={m['auc']:.6f} ap={m['average_precision']:.6f} logloss={m['log_loss']:.6f} "
            f"brier={m['brier']:.6f} ece10={m['ece_10']:.6f}"
        )


def print_regression_model_summary(name: str, metrics: dict[str, object]) -> None:
    print(f"model {name}")
    for split in ["train", "validation", "test"]:
        m = metrics[split]
        warning = " low_nonzero" if m.get("low_nonzero_warning") else ""
        print(
            f"  {split} n={m['n']} nonzero={m['nonzero']} "
            f"mean_y={m['mean_target']:.4f} mean_pred={m['mean_pred']:.4f} "
            f"rmse={m['rmse']:.4f} mae={m['mae']:.4f} pearson={m['pearson']:.6f} "
            f"auc_nonzero={m['rank_auc_nonzero']:.6f} ap_nonzero={m['average_precision_nonzero']:.6f}"
            f"{warning}"
        )
    by_kind = metrics.get("by_kind", {})
    if isinstance(by_kind, dict):
        for kind, split_metrics in by_kind.items():
            if not isinstance(split_metrics, dict):
                continue
            for split in ["train", "validation", "test"]:
                m = split_metrics.get(split)
                if not isinstance(m, dict):
                    continue
                warning = " low_nonzero" if m.get("low_nonzero_warning") else ""
                print(
                    f"  {split}/{kind} n={m['n']} nonzero={m['nonzero']} "
                    f"mean_y={m['mean_target']:.4f} mean_pred={m['mean_pred']:.4f} "
                    f"rmse={m['rmse']:.4f} pearson={m['pearson']:.6f} "
                    f"auc_nonzero={m['rank_auc_nonzero']:.6f} ap_nonzero={m['average_precision_nonzero']:.6f}"
                    f"{warning}"
                )


def split_for_row(row: dict[str, str], row_index: int = 0) -> str:
    if any(row.get(key) not in (None, "") for key in ["pid", "game_id", "search_id"]):
        return split_for(row.get("pid", ""), row.get("game_id", ""), row.get("search_id", ""))
    return split_for(row.get("decision_kind", ""), row.get("depth", ""), row.get("ply", ""), row_index)


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
