# Criticality Training Guide

How to collect shadow counterfactual probe data, train the LMR criticality
logistic model, and update the engine's coefficients — all from one script.

## What this is

Boa's late-move reductions use a **learned criticality guard**.  Before reducing
a quiet move's search depth, the engine computes a logistic score from 27
positional and search-state features.  If the score exceeds the P97 threshold
(meaning the model thinks the reduction is unusually risky), the engine
protects the move by restoring one ply of depth.

The model is trained on **shadow counterfactual probes**: occasionally the
engine searches a pruned or reduced move to full depth to see whether the
reduction changed the score.  Those "what-if" outcomes are the training labels.

This replaces ad-hoc pruning guards with a data-driven one.  The same
counterfactual-probe infrastructure works for futility pruning and reverse
futility pruning — this guide focuses on LMR, which is the first (and so
far most successful) application.

## Quick start

```sh
# 1. Build the engine
cargo build --release

# 2. Full pipeline: play 200 self-play games, train, write coefficients
python3 tools/train.py all --games 200 --probe-permille 5

# 3. The coefficients are written to target/release/criticality.coeffs
#    The engine loads them automatically on next launch.
```

If you already have probe data and only need to retrain:

```sh
python3 tools/train.py train --data analysis/criticality/20260628_120000/raw
```

## Commands

### `collect` — gather probe data

```
python3 tools/train.py collect [options]
```

Runs Boa-vs-Boa games through cutechess-cli with counterfactual probes enabled.
Output lands in `analysis/criticality/<timestamp>/`:

- `raw/criticality-*.csv.gz` — compressed probe rows (one shard per engine
  process)
- `games.pgn` — the self-play games
- `cutechess.log` — cutechess output

Key options:

| Option | Default | Notes |
|--------|---------|-------|
| `--games N` | 200 | Scored games (must be even) |
| `--tc VALUE` | `1+0.01` | cutechess time control |
| `--probe-permille N` | 5 | Shadow probe rate (5 = 0.5%) |
| `--concurrency N` | auto | Parallel games, capped at 12 |
| `--engine PATH` | `target/release/boa` | Engine binary |
| `--run-dir DIR` | `analysis/criticality/<ts>` | Output directory |

200 games at probe rate 5 produces ~40,000–60,000 shadow LMR rows.  That is
enough for a stable logistic fit.  For a quicker smoke test use 50 games.

### `train` — fit model and write coefficients

```
python3 tools/train.py train --data DIR [--out PATH] [--features ...]
```

Reads `criticality-*.csv.gz` shards from `DIR`, keeps only shadow
(`counterfactual_probe`) LMR rows, splits 70/15/15 by game ID, fits a
logistic regression, and writes a versioned `.coeffs` file.

| Option | Default | Notes |
|--------|---------|-------|
| `--data DIR` | *(required)* | Directory of compressed CSV shards |
| `--out PATH` | `target/release/criticality.coeffs` | Output path |
| `--features f1 f2 ...` | all 27 | Subset of features to use |

The output file archives the previous coefficients below a `---` separator
so nothing is ever lost.

### `all` — collect + train

```
python3 tools/train.py all [--games N] [--out PATH] ...
```

Runs `collect` then `train` in one shot.  All `collect` and `train` options
are accepted.

### `check` — probe health

```
python3 tools/train.py check --data DIR
```

Prints aggregate counts without training: total rows, per-kind breakdown,
label source distribution, and an estimate of how many trainable shadow LMR
rows are available.  Use this to verify a collection run before training.

## The .coeffs file

### Format

```
# Boa LMR criticality coefficients
# Generated: 2026-06-28T14:30:00+00:00
# Rows: 45231  |  Test AUC: 0.8231  |  Threshold: P97 = -4.54968
# Feature count: 27
intercept = -4.531881428371637
threshold = -4.54967678864419
root_depth = -0.15287501260142725
...
piece_king = 0.13015650867369016
---
# [2026-06-27T10:15:00+00:00]  rows=42000  test_auc=0.8153  p97_threshold=-4.60
# intercept = -4.52
...
```

- **Active section** (before `---`): plain `key = value` lines.  Parsed by the
  engine at startup.  Blank lines and `#`-prefixed lines are ignored.
- **History section** (after `---`): every line is commented out.  Human-readable
  archive of every previous training run.  Never parsed.

The engine loads `criticality.coeffs` from the same directory as the
executable (`target/release/`).  If the file is missing or malformed the
engine falls back to the hardcoded coefficients in `constants.rs`.

### Threshold

The threshold is the weighted P97 score on the validation split.  Moves whose
criticality logit exceeds this threshold get one ply of reduction protection.
P97 means ~3% of reduced moves are protected.

To use a different percentile, change `_SCORE_PERCENTILES` in `train.py` and
update the `threshold` key in the active section.  The engine just reads
the `threshold` value from the file — it does not care which percentile it
came from.

## Adding a feature

Say you want the model to also consider whether the previous move was a
capture.  You need to touch three files.

### 1. `tools/train.py` — add the feature name

Find `FEATURES` near the top of the file:

```python
FEATURES: list[str] = [
    "root_depth",
    ...
    "piece_king",
    "prev_move_is_capture",   # ← add here (order matters)
]
```

If the new feature needs custom normalisation (it's not a score / depth /
history / bool), add a branch in `_feature_value()`.

### 2. `src/search/pruning/lmr.rs` — add the feature computation

Update `FEATURE_COUNT`, `FEATURE_NAMES`, and the feature array in
`compute_score_from_coeffs`:

```rust
const FEATURE_COUNT: usize = 28;  // was 27

const FEATURE_NAMES: [&str; FEATURE_COUNT] = [
    ...
    "piece_king",
    "prev_move_is_capture",  // ← add here (same order as train.py)
];

fn compute_score_from_coeffs(...) -> f64 {
    let feat: [f64; FEATURE_COUNT] = [
        ...
        bool_feature(piece == PieceType::King),
        bool_feature(prev_move_is_capture),  // ← add here
    ];
    ...
}
```

Also update `legacy_criticality_score` with a `+ 0.0 * bool_feature(...)`
line so the hardcoded fallback compiles until you retrain.

### 3. Engine CSV emitter — add the column

Find where the engine writes criticality CSV rows (search for
`CriticalityRecord` in `alpha_beta.rs` and the CSV writer).  Add the new
column so the training script can read it.

### 4. Retrain

```sh
python3 tools/train.py all --games 200
```

The new `.coeffs` will include the new feature's coefficient.  Previous
coefficients are archived below `---`.

## Removing a feature

1. Remove the feature name from `FEATURES` in `train.py`.
2. Remove it from `FEATURE_NAMES` and the feature array in `lmr.rs`.
3. Decrement `FEATURE_COUNT`.
4. Remove the column from the engine's CSV emitter (or leave it — the
   training script ignores unrecognised columns).
5. Retrain.

The engine will warn about unrecognised keys in the `.coeffs` file if the
file still has the old feature.  That warning is harmless but it's cleanest
to retrain.

## Architecture decisions

**Why shadow-only?**  Observed-research labels (taken during normal search
with TT probe scores) are biased — the engine only probes moves it already
thinks are interesting.  Shadow counterfactual probes are unbiased: they fire
randomly at a fixed rate regardless of the engine's opinion about the move.
This gives cleaner training signal.

**Why P97 and not a continuous scale?**  The logistic score is a ranker, not
a calibrated probability.  Using it as a binary guard (protect if score ≥ P97)
is robust to calibration drift.  Continuous scaling (e.g. reducing less when
the score is higher) was tested and did not help — the score is too noisy at
intermediate values.

**Why compressed CSV and not Parquet?**  Compressed CSV is stdlib-only on the
Rust side (the engine writes it directly).  Parquet adds a heavy dependency
(pyarrow) to the training pipeline for modest space savings on datasets that
are typically under 200 MB.  If you need Parquet, `train_criticality.py`
still supports it via `--data` pointing at a Parquet directory.

**Why not put the training script in the engine binary?**  Separating
training from the engine means the engine binary stays small, the training
dependencies (numpy, scikit-learn) don't bloat the build, and you can train
on a different machine from the one that runs the engine.

## Troubleshooting

**"no labeled shadow LMR rows"**
Your probe rate is too low or the data has no counterfactual_probe labels.
Check with `train.py check --data DIR` first.  Try increasing
`--probe-permille` (max is 1000 = 100%).

**"too few positive examples"**
The model needs at least 20 bound_changed=1 examples in both train and
validation splits.  Collect more games.  A 200-game run at 5‰ should
produce ~100–200 positives.

**Engine doesn't seem to use new coefficients**
Make sure `criticality.coeffs` is in the same directory as the engine
binary (`target/release/`).  The engine looks for `./criticality.coeffs`
relative to the executable, not the current working directory.

**"unrecognised keys" warning**
The `.coeffs` file has features the engine doesn't know about.  This
happens when you retrain with new features but haven't updated the Rust
side yet, or when you remove a feature from `FEATURES` but the `.coeffs`
was trained with the old set.  Retrain to clear the warning.

## File reference

| File | Role |
|------|------|
| `tools/train.py` | Unified entry point (collect, train, all, check) |
| `tools/train_criticality.py` | Training library (logistic fit, metrics, CSV/Parquet loading) |
| `tools/criticality_dataset.mjs` | Game runner (cutechess orchestration, probe config) |
| `tools/openings.epd` | Opening book for self-play games |
| `criticality.coeffs` | Current coefficients + version history |
| `src/search/pruning/lmr.rs` | LMR reduction logic, coefficient loading, criticality score |
| `src/search/constants.rs` | Hardcoded fallback coefficients |
| `src/search/alpha_beta.rs` | Search loop, probe decision, CSV emission |
