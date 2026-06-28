#!/usr/bin/env python3
"""
Unified criticality training pipeline for Boa.

One entry point for the entire LMR shadow-P97 training loop:

    tools/train.py collect   – run self-play games with counterfactual probes
    tools/train.py train     – train model and write versioned .coeffs
    tools/train.py all       – collect + train in one shot
    tools/train.py check     – probe health / summary stats

Design
  • Only counterfactual_probe (shadow) rows are used for training.
  • The P97 threshold is chosen from the weighted validation-score percentile.
  • Coefficients are written to a .coeffs file that the engine parses at startup.
    Previous coefficients are archived as commented-out blocks so nothing is lost.
  • The feature set lives in one constant at the top of this file.  Add features
    by updating FEATURES (here) and the engine's CSV emitter + criticality scorer.

Requirements
  • python3 with numpy and scikit-learn
  • cargo build --release (for data collection)
  • cutechess-cli (for data collection)
  • Node.js (the game runner is in criticality_dataset.mjs)
"""

from __future__ import annotations

import argparse
import csv
import gzip
import hashlib
import json
import math
import os
import re
import subprocess
import sys
import textwrap
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

import numpy as np

# ═══════════════════════════════════════════════════════════════════════════════
# Feature configuration
#
# This is the single source of truth for the criticality feature set.
# To add a new feature:
#   1. Add its name to FEATURES below (order matters – must match the engine).
#   2. Add normalisation in _feature_value() if itʼs a new column type.
#   3. The engine must emit the column in the criticality CSV.
#   4. The engine must multiply it by the corresponding coefficient in
#      criticality_score().
#   5. Retrain – the .coeffs file updates automatically.
# ═══════════════════════════════════════════════════════════════════════════════

FEATURES: list[str] = [
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
    "piece_pawn",
    "piece_knight",
    "piece_bishop",
    "piece_rook",
    "piece_queen",
    "piece_king",
]

FEATURE_COUNT: int = len(FEATURES)

# Normalisation constants (must match the engine's normalisation).
_SCORE_CLIP: float = 2000.0
_HISTORY_CLIP: float = 16384.0
_DEPTH_DIVISOR: float = 16.0
_PLY_DIVISOR: float = 32.0
_MOVE_INDEX_DIVISOR: float = 32.0
_REDUCTION_DIVISOR: float = 4.0

# Score percentiles for threshold candidates.  P97 is the canonical choice.
_SCORE_PERCENTILES: list[float] = [50.0, 75.0, 90.0, 95.0, 97.0, 98.0, 99.0, 99.5]

# Smallest tolerable number of positive examples per split.
_MIN_POSITIVES: int = 20

# Default probe permille for shadow counterfactual probes.
_DEFAULT_PROBE_PERMILLE: int = 5

# ═══════════════════════════════════════════════════════════════════════════════
# CLI
# ═══════════════════════════════════════════════════════════════════════════════


def _script_dir() -> Path:
    return Path(__file__).resolve().parent


def _repo_root() -> Path:
    return _script_dir().parent


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = parser.add_subparsers(dest="command", title="subcommands")

    # ---- collect ----
    p_collect = sub.add_parser("collect", help="Run self-play games collecting shadow probes")
    p_collect.add_argument("--games", type=int, default=200, help="Scored games (default: 200)")
    p_collect.add_argument("--tc", default="1+0.01", help="Time control (default: 1+0.01)")
    p_collect.add_argument("--concurrency", type=int, default=0,
                           help="Parallel games (default: auto, capped at 12)")
    p_collect.add_argument("--probe-permille", type=int, default=_DEFAULT_PROBE_PERMILLE,
                           help=f"Probe rate (default: {_DEFAULT_PROBE_PERMILLE})")
    p_collect.add_argument("--engine", default=None, help="Boa binary (default: target/release/boa)")
    p_collect.add_argument("--cutechess", default=None, help="cutechess-cli path")
    p_collect.add_argument("--openings", default=None, help="Opening EPD file")
    p_collect.add_argument("--run-dir", default=None,
                           help="Output directory (default: analysis/criticality/<timestamp>)")

    # ---- train ----
    p_train = sub.add_parser("train", help="Train model from existing probe data")
    p_train.add_argument("--data", required=True, help="Directory containing criticality-*.csv.gz shards")
    p_train.add_argument("--out", default=None,
                         help=".coeffs output path (default: target/release/criticality.coeffs)")
    p_train.add_argument("--features", nargs="*", default=None,
                         help="Feature subset to use (default: all configured features)")

    # ---- all ----
    p_all = sub.add_parser("all", help="collect + train")
    p_all.add_argument("--games", type=int, default=200)
    p_all.add_argument("--tc", default="1+0.01")
    p_all.add_argument("--concurrency", type=int, default=0)
    p_all.add_argument("--probe-permille", type=int, default=_DEFAULT_PROBE_PERMILLE)
    p_all.add_argument("--engine", default=None)
    p_all.add_argument("--cutechess", default=None)
    p_all.add_argument("--openings", default=None)
    p_all.add_argument("--run-dir", default=None)
    p_all.add_argument("--out", default=None, help=".coeffs output path")

    # ---- check ----
    p_check = sub.add_parser("check", help="Summarise probe data without training")
    p_check.add_argument("--data", required=True, help="Directory containing criticality-*.csv.gz shards")

    args = parser.parse_args()

    if args.command == "collect":
        _cmd_collect(args)
    elif args.command == "train":
        _cmd_train(args)
    elif args.command == "all":
        run_dir = _cmd_collect(args)
        # Patch args so train uses the collected data.
        args.data = str(run_dir / "raw")
        if args.out is None:
            args.out = str(_repo_root() / "target" / "release" / "criticality.coeffs")
        _cmd_train(args)
    elif args.command == "check":
        _cmd_check(args)
    else:
        parser.print_help()


# ═══════════════════════════════════════════════════════════════════════════════
# collect
# ═══════════════════════════════════════════════════════════════════════════════


def _cmd_collect(args) -> Path:
    """Run the Node.js game runner and return the run directory."""
    runner = _script_dir() / "criticality_dataset.mjs"
    if not runner.exists():
        raise SystemExit(f"game runner not found: {runner}")

    engine = args.engine or str(_repo_root() / "target" / "release" / "boa")
    cutechess = args.cutechess or "tools/cutechess-cli"
    openings = args.openings or str(_script_dir() / "openings.epd")
    run_dir = args.run_dir or str(
        _repo_root() / "analysis" / "criticality" / _timestamp_str()
    )

    cmd: list[str] = [
        "node", str(runner),
        "--engine", engine,
        "--cutechess", cutechess,
        "--openings", openings,
        "--run-dir", run_dir,
        "--games", str(args.games),
        "--tc", args.tc,
        "--probe-permille", str(args.probe_permille),
    ]
    if args.concurrency:
        cmd.extend(["--concurrency", str(args.concurrency)])

    print(f"[collect] {' '.join(cmd)}", flush=True)
    result = subprocess.run(cmd)
    if result.returncode != 0:
        raise SystemExit(result.returncode)

    run_path = Path(run_dir)
    raw = run_path / "raw"
    shards = sorted(raw.glob("criticality-*.csv.gz")) if raw.is_dir() else []
    if shards:
        print(f"[collect] {len(shards)} compressed CSV shard(s) written to {raw}")
    else:
        print(f"[collect] warning: no criticality-*.csv.gz found in {raw} — "
              f"check cutechess.log in {run_dir}")
    return run_path


# ═══════════════════════════════════════════════════════════════════════════════
# Data loading
# ═══════════════════════════════════════════════════════════════════════════════


def _load_shadow_rows(csv_dir: Path, features: list[str] | None = None) -> list[dict]:
    """Load only shadow (label_source=default_counterfactual_probe) LMR rows.

    Returns rows with normalised float features, integer label, and split.
    """
    shards = sorted(csv_dir.glob("criticality-*.csv")) + sorted(csv_dir.glob("criticality-*.csv.gz"))
    if not shards:
        raise SystemExit(f"no criticality-*.csv[.gz] shards found in {csv_dir}")

    rows: list[dict] = []
    required_cols = {
        "decision_kind", "bound_changed", "label_source",
        "pid", "game_id", "search_id",
    } | (set(features or FEATURES))
    # The CSV may also contain has_prev_static_eval as a separate column,
    # but we derive it from prev_static_eval if missing.
    stats = {"lmr": 0, "shadow": 0, "labeled": 0, "total": 0}

    for shard in shards:
        opener = gzip.open if shard.name.endswith(".gz") else open
        with opener(shard, "rt", newline="", encoding="utf8", errors="replace") as handle:
            reader = csv.DictReader(handle)
            if reader.fieldnames is None:
                continue
            missing = required_cols - set(reader.fieldnames)
            if missing:
                print(f"[load] warning: {shard.name} missing columns: {missing}", file=sys.stderr)
                continue

            for row in reader:
                stats["total"] += 1
                kind = (row.get("decision_kind") or "").strip().lower()
                if kind not in ("lmr", ""):
                    continue
                stats["lmr"] += 1

                source = (row.get("label_source") or "").strip()
                # The engine writes "default_counterfactual_probe" for shadow probes.
                # Accept both that and the legacy "counterfactual_probe".
                if source not in ("counterfactual_probe", "default_counterfactual_probe"):
                    continue
                stats["shadow"] += 1

                label_str = row.get("bound_changed", "")
                if label_str not in ("0", "1"):
                    continue
                stats["labeled"] += 1

                # Build row dict with normalised features + metadata.
                r: dict = {
                    "label": int(label_str),
                    "split": _split_for(
                        row.get("pid", ""),
                        row.get("game_id", ""),
                        row.get("search_id", ""),
                    ),
                }
                r.update(_build_feature_dict(row, features or FEATURES))
                rows.append(r)

    if stats["labeled"] == 0:
        raise SystemExit(
            f"no labeled shadow LMR rows in {csv_dir}. "
            f"total={stats['total']} lmr={stats['lmr']} shadow={stats['shadow']}"
        )
    print(
        f"[load] total={stats['total']} lmr={stats['lmr']} "
        f"shadow={stats['shadow']} labeled={stats['labeled']} "
        f"pos_rate={sum(1 for r in rows if r['label']) / len(rows):.4f}"
    )
    return rows


def _build_feature_dict(row: dict[str, str], features: list[str]) -> dict[str, float]:
    out: dict[str, float] = {}
    raw = {k: _parse_float(row.get(k, "")) for k in features}
    for name in features:
        out[name] = _feature_value(name, raw)
    return out


def _feature_value(name: str, raw: dict[str, float]) -> float:
    """Normalise a single feature exactly as the engine does."""
    value = raw.get(name, 0.0)

    if name == "side_to_move_black":
        # Derived column: the CSV has side_to_move as "white"/"black".
        return 1.0 if raw.get("side_to_move", 0.0) != 0.0 else 0.0

    if name.startswith("piece_"):
        piece = name.removeprefix("piece_")
        # The CSV column is `piece_type` with values "p","n","b","r","q","k".
        return 1.0 if raw.get("piece_type", 0.0) != 0.0 else 0.0

    if name == "history_score":
        return float(np.clip(value, -_HISTORY_CLIP, _HISTORY_CLIP) / _HISTORY_CLIP)

    if name in {
        "static_eval", "prev_static_eval", "static_eval_delta",
        "alpha", "beta", "volatility", "king_danger",
        "planned_margin", "gap",
    }:
        return float(np.clip(value, -_SCORE_CLIP, _SCORE_CLIP) / _SCORE_CLIP)

    if name in ("root_depth", "depth", "new_depth", "null_depth"):
        return value / _DEPTH_DIVISOR

    if name == "ply":
        return value / _PLY_DIVISOR

    if name == "move_index":
        return value / _MOVE_INDEX_DIVISOR

    if name in ("base_reduction", "final_reduction", "planned_reduction", "null_reduction"):
        return value / _REDUCTION_DIVISOR

    # Boolean features (is_pv, is_cut_node, improving, is_killer, is_counter,
    # tt_move_agreement, has_prev_static_eval) are already 0/1 in the CSV.
    # node_type is 0/1/2.
    return value


def _parse_float(raw: str | None) -> float:
    if raw in (None, ""):
        return 0.0
    try:
        return float(raw)
    except ValueError:
        return 0.0


def _split_for(*parts: object) -> str:
    digest = hashlib.blake2b(
        ":".join(str(p) for p in parts).encode("ascii"), digest_size=4
    ).digest()
    bucket = int.from_bytes(digest, "little") % 10
    if bucket == 0:
        return "test"
    if bucket == 1:
        return "validation"
    return "train"


# ═══════════════════════════════════════════════════════════════════════════════
# Training
# ═══════════════════════════════════════════════════════════════════════════════


def _train_model(
    rows: list[dict],
    features: list[str],
) -> dict:
    """Train logistic regression on shadow counterfactual probe data.

    Returns a dict with:
        intercept: float
        threshold: float          # validation P97 score
        coefficients: dict[str, float]
        metrics: dict             # train/val/test AUC, logloss, etc.
        percentile_table: dict    # all threshold candidates
        training_info: dict       # metadata for the .coeffs header
    """
    from sklearn.linear_model import LogisticRegression

    X = np.array([[r[f] for f in features] for r in rows], dtype=np.float64)
    y = np.array([r["label"] for r in rows], dtype=np.float64)
    splits = np.array([r["split"] for r in rows])

    train_mask = splits == "train"
    val_mask = splits == "validation"
    test_mask = splits == "test"

    if train_mask.sum() == 0 or val_mask.sum() == 0:
        # Fallback: manually split if the deterministic split didn't produce
        # enough validation rows.
        print("[train] deterministic split missing a fold, falling back to manual 70/15/15")
        indices = np.random.RandomState(42).permutation(len(rows))
        n = len(rows)
        t_end = int(n * 0.70)
        v_end = int(n * 0.85)
        train_mask = np.zeros(n, dtype=bool)
        val_mask = np.zeros(n, dtype=bool)
        test_mask = np.zeros(n, dtype=bool)
        train_mask[indices[:t_end]] = True
        val_mask[indices[t_end:v_end]] = True
        test_mask[indices[v_end:]] = True
        for i, m in enumerate(train_mask):
            rows[i]["split"] = "train" if m else ("validation" if val_mask[i] else "test")

    X_train, y_train = X[train_mask], y[train_mask]
    X_val, y_val = X[val_mask], y[val_mask]
    X_test, y_test = X[test_mask], y[test_mask]

    pos_train = int(y_train.sum())
    pos_val = int(y_val.sum())
    pos_test = int(y_test.sum())
    if pos_train < _MIN_POSITIVES or pos_val < _MIN_POSITIVES:
        raise SystemExit(
            f"too few positive examples: train={pos_train} val={pos_val}. "
            f"Collect more games or increase probe rate."
        )

    print(
        f"[train] splits: train={len(y_train)} (pos={pos_train}) "
        f"val={len(y_val)} (pos={pos_val}) test={len(y_test)} (pos={pos_test})"
    )

    # Fit logistic regression with balanced class weights.
    model = LogisticRegression(
        penalty="l2",
        C=1.0,
        solver="lbfgs",
        max_iter=500,
        class_weight="balanced",
        random_state=42,
    )
    model.fit(X_train, y_train)

    # Extract raw-feature coefficients.
    intercept = float(model.intercept_[0])
    coeffs = {name: float(w) for name, w in zip(features, model.coef_[0])}

    # Compute decision scores (logits) for threshold selection.
    scores_train = model.decision_function(X_train)
    scores_val = model.decision_function(X_val)
    scores_test = model.decision_function(X_test)

    # Threshold: weighted P97 of validation scores.
    thresholds = _weighted_percentiles(scores_val, np.ones_like(scores_val))
    threshold = thresholds.get("p97", float(np.percentile(scores_val, 97)))

    # Predictions for metrics.
    proba_train = model.predict_proba(X_train)[:, 1]
    proba_val = model.predict_proba(X_val)[:, 1]
    proba_test = model.predict_proba(X_test)[:, 1]

    def _metrics(y_true, proba, scores):
        return {
            "n": int(len(y_true)),
            "positives": int(y_true.sum()),
            "positive_rate": float(y_true.mean()),
            "auc": float(_roc_auc(y_true, scores)),
            "average_precision": float(_avg_precision(y_true, scores)),
            "log_loss": float(-np.mean(y_true * np.log(np.clip(proba, 1e-12, 1)))
                               - np.mean((1 - y_true) * np.log(np.clip(1 - proba, 1e-12, 1)))),
        }

    metrics = {
        "train": _metrics(y_train, proba_train, scores_train),
        "validation": _metrics(y_val, proba_val, scores_val),
        "test": _metrics(y_test, proba_test, scores_test),
        "percentile_table": {
            f"p{p:g}".replace(".", "_"): float(v)
            for p, v in _weighted_percentiles(scores_val, np.ones_like(scores_val)).items()
        },
        "feature_count": len(features),
    }

    print(f"[train] test AUC={metrics['test']['auc']:.4f}  "
          f"AP={metrics['test']['average_precision']:.4f}  "
          f"P97 threshold={threshold:.6f}")

    return {
        "intercept": intercept,
        "threshold": threshold,
        "coefficients": coeffs,
        "metrics": metrics,
        "training_info": {
            "total_rows": len(rows),
            "train_rows": int(train_mask.sum()),
            "val_rows": int(val_mask.sum()),
            "test_rows": int(test_mask.sum()),
            "positives_train": pos_train,
            "positives_val": pos_val,
            "positives_test": pos_test,
        },
    }


def _weighted_percentiles(
    values: np.ndarray, weights: np.ndarray
) -> dict[str, float]:
    order = np.argsort(values)
    v = values[order]
    w = weights[order]
    total = float(w.sum())
    if total <= 0:
        return {f"p{p:g}".replace(".", "_"): float(np.percentile(values, p))
                for p in _SCORE_PERCENTILES}
    cdf = np.cumsum(w) / total
    out: dict[str, float] = {}
    for p in _SCORE_PERCENTILES:
        idx = min(int(np.searchsorted(cdf, p / 100.0, side="left")), len(v) - 1)
        out[f"p{p:g}".replace(".", "_")] = float(v[idx])
    return out


def _roc_auc(y: np.ndarray, scores: np.ndarray) -> float:
    """Weighted ROC AUC (unweighted here since shadow probes are IID)."""
    order = np.argsort(scores, kind="mergesort")
    y_sorted = y[order]
    # Count positives and negatives.
    pos = int(y.sum())
    neg = len(y) - pos
    if pos == 0 or neg == 0:
        return float("nan")

    # TPR and FPR as we walk the sorted scores.
    tp = 0
    fp = 0
    auc = 0.0
    prev_fpr = 0.0
    i = 0
    while i < len(y_sorted):
        j = i
        while j < len(y_sorted) and scores[order[j]] == scores[order[i]]:
            j += 1
        group_y = y_sorted[i:j]
        group_pos = int(group_y.sum())
        group_neg = j - i - group_pos
        tp += group_pos
        fp += group_neg
        tpr = tp / pos
        fpr = fp / neg
        auc += tpr * (fpr - prev_fpr)
        prev_fpr = fpr
        i = j
    return auc


def _avg_precision(y: np.ndarray, scores: np.ndarray) -> float:
    order = np.argsort(-scores, kind="mergesort")
    y_sorted = y[order]
    pos_total = int(y.sum())
    if pos_total == 0:
        return float("nan")
    tp = 0
    fp = 0
    ap = 0.0
    i = 0
    while i < len(y_sorted):
        j = i
        while j < len(y_sorted) and scores[order[j]] == scores[order[i]]:
            j += 1
        group_y = y_sorted[i:j]
        group_pos = int(group_y.sum())
        tp += group_pos
        fp += j - i - group_pos
        ap += group_pos * tp / (tp + fp) if (tp + fp) > 0 else 0.0
        i = j
    return ap / pos_total


# ═══════════════════════════════════════════════════════════════════════════════
# .coeffs file management
# ═══════════════════════════════════════════════════════════════════════════════

_COEFFS_SEPARATOR = "---"


def _write_coeffs(
    path: Path,
    intercept: float,
    threshold: float,
    coefficients: dict[str, float],
    features: list[str],
    info: dict,
    metrics: dict,
) -> None:
    """Write a .coeffs file with versioned history.

    Active coefficients appear first as plain key=value lines.
    Everything after the ``---`` separator is commented-out history.
    Previous active coefficients are archived before the new ones are written.
    """
    now = datetime.now(timezone.utc).isoformat(timespec="seconds")
    test_auc = metrics["test"]["auc"]
    n_rows = info["total_rows"]

    # Read any existing history.
    existing_history: str = ""
    if path.exists():
        old_text = path.read_text(encoding="utf8", errors="replace")
        sep_idx = old_text.find(_COEFFS_SEPARATOR)
        if sep_idx >= 0:
            # Keep existing history, stripping any trailing whitespace.
            existing_history = "\n" + old_text[sep_idx:].strip()

    # Build the new history entry (commented out).
    hist_lines = [
        f"# [{now}]  rows={n_rows}  test_auc={test_auc:.4f}  "
        f"p97_threshold={threshold:.6f}",
        f"# intercept = {intercept:.12g}",
        f"# threshold = {threshold:.12g}",
    ]
    for name in features:
        w = coefficients.get(name, 0.0)
        hist_lines.append(f"# {name} = {w:.12g}")

    new_history_block = "\n".join(hist_lines)

    # Build the active section.
    active_lines = [
        f"# Boa LMR criticality coefficients",
        f"# Generated: {now}",
        f"# Rows: {n_rows}  |  Test AUC: {test_auc:.4f}  |  "
        f"Threshold: P97 = {threshold:.6f}",
        f"# Feature count: {len(features)}",
        f"intercept = {intercept:.12g}",
        f"threshold = {threshold:.12g}",
    ]
    for name in features:
        w = coefficients.get(name, 0.0)
        active_lines.append(f"{name} = {w:.12g}")

    active_block = "\n".join(active_lines)

    # Compose: active + separator + new history + old history.
    parts = [active_block, _COEFFS_SEPARATOR, new_history_block]
    if existing_history.strip():
        parts.append(existing_history)
    parts.append("")  # trailing newline

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(parts), encoding="utf8")
    print(f"[coeffs] wrote {path} ({len(features)} features)")


def _parse_coeffs(path: Path) -> dict[str, float] | None:
    """Parse active coefficients from a .coeffs file.

    Only reads lines before the first ``---`` separator.  Returns a dict
    mapping feature name → coefficient, or None if the file is missing
    or unparseable.
    """
    if not path.exists():
        return None

    text = path.read_text(encoding="utf8", errors="replace")
    # Truncate at separator.
    sep_idx = text.find(_COEFFS_SEPARATOR)
    if sep_idx >= 0:
        text = text[:sep_idx]

    coeffs: dict[str, float] = {}
    key_re = re.compile(r"^([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*(-?[\d.eE+-]+)")
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        m = key_re.match(line)
        if m:
            try:
                coeffs[m.group(1)] = float(m.group(2))
            except ValueError:
                continue
    if not coeffs:
        return None
    return coeffs


# ═══════════════════════════════════════════════════════════════════════════════
# train command
# ═══════════════════════════════════════════════════════════════════════════════


def _cmd_train(args) -> None:
    data_dir = Path(args.data)
    if not data_dir.is_dir():
        raise SystemExit(f"not a directory: {data_dir}")

    features = args.features if args.features else FEATURES
    # Validate features.
    unknown = set(features) - set(FEATURES)
    if unknown:
        raise SystemExit(f"unknown features: {unknown}. Configured: {FEATURES}")

    print(f"[train] loading shadow LMR rows from {data_dir}")
    rows = _load_shadow_rows(data_dir, features)
    if not rows:
        raise SystemExit("no training rows loaded")

    result = _train_model(rows, features)

    out_path = Path(args.out) if args.out else _repo_root() / "target" / "release" / "criticality.coeffs"
    _write_coeffs(
        out_path,
        result["intercept"],
        result["threshold"],
        result["coefficients"],
        features,
        result["training_info"],
        result["metrics"],
    )

    # Print a compact summary so the user can copy coefficients into
    # constants.rs as a fallback if they choose.
    print()
    print("── Active coefficients ──")
    print(f"intercept = {result['intercept']:.12g}")
    print(f"threshold = {result['threshold']:.12g}")
    for name in features:
        print(f"{name} = {result['coefficients'][name]:.12g}")
    print(f"── Test AUC: {result['metrics']['test']['auc']:.4f} ──")


# ═══════════════════════════════════════════════════════════════════════════════
# check command
# ═══════════════════════════════════════════════════════════════════════════════


def _cmd_check(args) -> None:
    data_dir = Path(args.data)
    if not data_dir.is_dir():
        raise SystemExit(f"not a directory: {data_dir}")

    shards = sorted(data_dir.glob("criticality-*.csv")) + sorted(data_dir.glob("criticality-*.csv.gz"))
    if not shards:
        raise SystemExit(f"no criticality-*.csv[.gz] shards in {data_dir}")

    total_bytes = sum(p.stat().st_size for p in shards)
    from collections import Counter

    counts: Counter[str] = Counter()
    kind_counts: Counter[str] = Counter()
    source_counts: Counter[str] = Counter()
    label_counts: Counter[str] = Counter()
    total = 0

    for shard in shards:
        opener = gzip.open if shard.name.endswith(".gz") else open
        with opener(shard, "rt", newline="", encoding="utf8", errors="replace") as handle:
            reader = csv.DictReader(handle)
            if reader.fieldnames is None:
                continue
            for row in reader:
                total += 1
                kind = (row.get("decision_kind") or "?").strip().lower()
                source = (row.get("label_source") or "none").strip()
                label = row.get("bound_changed", "?")
                kind_counts[kind] += 1
                source_counts[source] += 1
                if label in ("0", "1"):
                    label_counts[label] += 1

    print(f"files:              {len(shards)}")
    print(f"total_size_mib:     {total_bytes / (1024 * 1024):.1f}")
    print(f"total_rows:         {total}")
    print(f"by_kind:            {dict(kind_counts)}")
    print(f"by_label_source:    {dict(source_counts)}")
    print(f"by_bound_changed:   {dict(label_counts)}")

    shadow = source_counts.get("counterfactual_probe", 0) + source_counts.get("default_counterfactual_probe", 0)
    lmr = kind_counts.get("lmr", 0)
    print(f"trainable:          {min(shadow, lmr)} (shadow LMR overlap estimate)")


# ═══════════════════════════════════════════════════════════════════════════════
# Utilities
# ═══════════════════════════════════════════════════════════════════════════════


def _timestamp_str() -> str:
    return datetime.now().strftime("%Y%m%d_%H%M%S")


if __name__ == "__main__":
    main()
