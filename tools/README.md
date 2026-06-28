# Boa Tools

Engine testing, match running, and criticality model training.

## Tool Map

- `train.py` — unified criticality pipeline (collect, train, all, check).
- `criticality_dataset.mjs` — game runner invoked by `train.py collect`.
- `train_criticality.py` — training library (logistic fit, metrics, CSV loading).
- `openings.epd` — opening suite for self-play and engine matches.
- `CRITICALITY_GUIDE.md` — how to use the training pipeline, add/remove features.
- `AGENT_GUIDE.md` — playbook for coding agents using these tools.

Generated files that should stay local: `target/`, `analysis/`.

## Requirements

Build the engine before running tools that execute Boa:

```sh
cargo build --release
```

Local dependencies:

- `cutechess-cli` — required for self-play game collection.
- `python3` with `numpy` and `scikit-learn` — required for training.
- `node` — required for the game runner (`criticality_dataset.mjs`).

## Criticality Training

The learned LMR criticality guard is trained on shadow counterfactual probes.
One script drives the whole pipeline:

```sh
# Full pipeline: play 200 self-play games, train, write coefficients
python3 tools/train.py all --games 200 --probe-permille 5

# Collect only:
python3 tools/train.py collect --games 200

# Train from existing data:
python3 tools/train.py train --data analysis/criticality/<run>/raw

# Probe health check:
python3 tools/train.py check --data analysis/criticality/<run>/raw
```

The coefficients are written to `target/release/criticality.coeffs`.  The
engine loads them at startup.  Previous coefficients are archived as
commented-out history below the `---` separator.

Full documentation: `tools/CRITICALITY_GUIDE.md`.

## Running Matches

Use `cutechess-cli` directly for scripted matches.  Keep both engines on the
same hash, openings, time control, adjudication, and concurrency.

Candidate vs baseline:

```sh
cutechess-cli \
  -engine cmd=target/release/boa proto=uci name=candidate option.Hash=64 \
  -engine cmd=<snapshot>/boa proto=uci name=baseline option.Hash=64 \
  -each proto=uci tc=5+0.05 \
  -games 2 -rounds 200 -repeat \
  -concurrency 8 \
  -openings file=tools/openings.epd format=epd order=random \
  -recover -maxmoves 200 \
  -draw movenumber=40 movecount=8 score=10 \
  -resign movecount=5 score=700 twosided=true
```

SPRT shape:

```sh
cutechess-cli \
  -engine cmd=target/release/boa proto=uci name=candidate option.Hash=64 \
  -engine cmd=<snapshot>/boa proto=uci name=baseline option.Hash=64 \
  -each proto=uci tc=1+0.01 \
  -games 2 -rounds 5000 -repeat \
  -concurrency 8 \
  -openings file=tools/openings.epd format=epd order=random \
  -recover -maxmoves 200 \
  -draw movenumber=40 movecount=8 score=10 \
  -resign movecount=5 score=700 twosided=true \
  -sprt elo0=0 elo1=5 alpha=0.05 beta=0.05
```

For PRs, record the command shape, time control, openings, game count, W-D-L,
Elo/error or SPRT result, and compared snapshots or commits.

## Openings

`tools/openings.epd` is the shared opening suite.  Use it for engine
comparisons unless the experiment specifically tests an opening book or a
narrow opening family.

Recommended cutechess arguments:

```sh
-openings file=tools/openings.epd format=epd order=random policy=round
```

Keep opening selection fixed across candidate and baseline runs.

## Release Workflow

Windows releases are published automatically when a version tag is pushed.
See `.github/workflows/release.yml`.
