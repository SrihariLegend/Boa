# Boa Tools

This directory contains the local tooling used to test Boa, run engine matches,
inspect restriction-style behavior, and manage release support files.

## Tool Map

- `openings.epd`: opening suite for fair engine comparisons.
- `player_style_probe.mjs`: fixed-depth style diagnostic against reference PGNs.
- `../games/*.zip`: zipped Karpov, Petrosian, and Keres PGN archives.
- `AGENT_GUIDE.md`: concise playbook for coding agents using these tools.

Generated files that should stay local:

Do not commit `target/` or `analysis/`.

## Requirements

Build the current engine before running tools that execute Boa:

```sh
cargo build --release
```

Local dependencies:

- `unzip`: required by PGN tools that read zipped archives.
- `fastchess` (bundled in [match-manager](https://github.com/user/match-manager)): required for matches.
- `stockfish` on `PATH` or `/usr/games/stockfish`: optional Stockfish matches.

## Match Manager

Match Manager has been extracted to its own standalone repository:

**[match-manager](https://github.com/user/match-manager)** — terminal match
manager for UCI chess engines.

It wraps `fastchess`, manages engine snapshots, runs approval matches and
scripted ablations, and provides both an interactive terminal UI and a web UI.

See the standalone repo for full documentation and setup instructions. The
ablation definitions and snapshot workflow have moved there.

## Direct fastchess

Use direct fastchess for scripted non-regression checks when the Match Manager
UI is not appropriate. fastchess is bundled in the standalone
[match-manager](https://github.com/user/match-manager) repo. Keep both engines
on the same hash, openings, time control, adjudication, and concurrency.

Example candidate vs saved baseline:

```sh
match-manager/fastchess -output cutechess \
  -engine cmd=target/release/boa proto=uci name=candidate option.Hash=64 \
  -engine cmd=<snapshot_path>/boa proto=uci name=baseline option.Hash=64 \
  -each proto=uci tc=5+0.05 \
  -games 2 -rounds 200 -repeat \
  -concurrency 8 \
  -openings file=tools/openings.epd format=epd order=random \
  -recover -maxmoves 200 \
  -draw movenumber=40 movecount=8 score=10 \
  -resign movecount=5 score=700 twosided=true
```

Example SPRT shape:

```sh
match-manager/fastchess -output cutechess \
  -engine cmd=target/release/boa proto=uci name=candidate option.Hash=64 \
  -engine cmd=<snapshot_path>/boa proto=uci name=baseline option.Hash=64 \
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

## Ablation Runner

The ablation runner is part of the standalone [match-manager](https://github.com/user/match-manager) repo.

It tests individual UCI-controlled features using the same engine snapshot as
both sides, with one side receiving an option override. The reported Elo is
from the ablated candidate's perspective. If `no_eval_freedom` loses clearly,
the freedom term is useful. If it wins clearly, the term is harmful or
overweighted. If it is inside the error bar, the result is unclear.

See the match-manager README for full usage, including `--list`, `--suite scale`,
`--sprt`, and interpretation details.

## Restriction Signal Dataset

Path: `tools/restriction_signal.mjs`

This is the Phase 0 research workflow. It samples positions from the bundled GM
archives, asks Boa for diagnostic restriction features, and labels each row with
the static eval four plies later. The generated CSV is local analysis output and
is ignored by git under `analysis/`.

Small smoke run:

```sh
cargo build --release
node tools/restriction_signal.mjs --positions 200 --stride 10 --progress 50
python3 tools/analyze_restriction_signal.py analysis/restriction_signal/gm_features.csv
```

Larger GM archive run:

```sh
node tools/restriction_signal.mjs \
  --positions 500000 \
  --stride 1 \
  --min-ply 12 \
  --max-ply 100 \
  --future-plies 4 \
  --out analysis/restriction_signal/gm_features.csv
```

Use `--quiet` when you want the sample restricted to positions where the side to
move is not in check and the next played move is not a capture or promotion.

## Texel Scale Tuning

Path: `tools/texel_tune.py`

This is the first Phase 1 tuning pass. It consumes quiet rows from
`restriction_signal.mjs` and tunes Boa's exposed UCI eval scale knobs against
game outcomes. By default it tunes the core terms plus trade-down:

- material
- PST
- mobility
- pawn structure
- king safety
- trade-down

It recommends setting the current restriction-style terms to zero during this
fit: freedom, weak squares, coordination, and advanced pawns. This keeps the
baseline tuning focused on the classical terms that Phase 0 showed were the
stronger signal.

Small smoke run:

```sh
cargo build --release
node tools/restriction_signal.mjs \
  --quiet \
  --positions 1000 \
  --stride 20 \
  --out analysis/restriction_signal/texel_smoke.csv
python3 tools/texel_tune.py analysis/restriction_signal/texel_smoke.csv --limit 1000
```

Full initial GM tuning run:

```sh
node tools/restriction_signal.mjs \
  --quiet \
  --positions 500000 \
  --stride 1 \
  --min-ply 12 \
  --max-ply 100 \
  --out analysis/restriction_signal/texel_quiet.csv
python3 tools/texel_tune.py analysis/restriction_signal/texel_quiet.csv
```

Treat the output as candidate UCI options first. Validate them with a focused
SPRT or non-regression match before changing defaults in code.

The first GM-outcome scale candidate improved quiet-position MSE but failed
SPRT against the current defaults:

```text
tuned vs default: 718 - 879 - 275 [0.457] 1872
Elo difference: -30.0 +/- 14.6, LOS: 0.0%, H0 accepted
tuned as White: 379 - 406 - 151 [0.486]
tuned as Black: 339 - 473 - 124 [0.428]
```

Do not ship that scale set as defaults. Treat GM-outcome tuning as a diagnostic
fit, not a strength proxy.

## Internal-Weight Texel Tuning

Path: `tools/texel_tune_internal.py`

This tuner consumes the self-play CSV from `tools/self_play_dataset.mjs` and
fits internal eval constants directly. It decomposes FENs into sparse
coefficients for the current non-PST eval terms:

- mobility and activity tables
- pawn structure and passed-pawn terms
- king safety
- freedom/squeeze terms
- trade-down, weak-square, coordination, and advanced-pawn terms

Run a constrained smoke fit for one group:

```sh
python3 tools/texel_tune_internal.py \
  analysis/self_play/texel_self_play.csv \
  --groups mobility \
  --limit 2000 \
  --steps 4,2 \
  --passes 1
```

Run a larger constrained fit for one group:

```sh
python3 tools/texel_tune_internal.py \
  analysis/self_play/texel_self_play.csv \
  --groups mobility \
  --limit 100000 \
  --steps 8,4,2,1 \
  --passes 2 \
  > analysis/self_play/internal_tune_mobility.txt
```

The script prints reconstruction error for its internal model. Mean absolute
error should stay near centipawn rounding noise before the tuned values are
trusted. Treat the emitted Rust replacements as a candidate report, not as
ship-ready eval defaults.

By default the internal tuner now uses:

- an L2 prior around current constants (`--l2`)
- a deterministic train/validation split (`--validation-fraction`)
- a bounded parameter window (`--max-delta`)
- semantic and monotonic constraints
- a validation gate that rejects updates unless holdout MSE improves

Prefer group-by-group fits (`--groups mobility`, `--groups pawn`, etc.) and
validate each candidate with SPRT or a non-regression match before landing it in
`eval.rs`. Use `--no-constraints` and `--no-validation-gate` only for diagnostics;
unconstrained full-table result-label fits have already failed transfer to
playing strength.

## Self-Play Texel Dataset

Path: `tools/self_play_dataset.mjs`

This wrapper generates Boa-vs-Boa games with cutechess and then feeds the PGN
through `restriction_signal.mjs`. Use it for Phase 1 tuning data that matches
the positions Boa actually reaches in play.

Small smoke run:

```sh
cargo build --release
node tools/self_play_dataset.mjs \
  --games 20 \
  --quiet \
  --positions 1000 \
  --out analysis/self_play/smoke.csv
python3 tools/texel_tune.py analysis/self_play/smoke.csv --limit 1000
```

Larger self-play extraction:

```sh
node tools/self_play_dataset.mjs \
  --games 5000 \
  --tc 1+0.01 \
  --concurrency 12 \
  --quiet \
  --positions 500000 \
  --out analysis/self_play/texel_self_play.csv
python3 tools/texel_tune.py analysis/self_play/texel_self_play.csv
```

For very large runs, generate the PGN once and reuse it while iterating on
extraction parameters:

```sh
node tools/self_play_dataset.mjs \
  --skip-games \
  --pgn analysis/self_play/self_play.pgn \
  --quiet \
  --out analysis/self_play/texel_self_play.csv
```

## Internal PST Tuning

Path: `tools/texel_tune_pst.py`

This is the first internal-weight tuning slice. It tunes only pawn and knight
PST midgame/endgame entries while keeping the rest of the eval fixed. It uses
the self-play CSV directly and treats `white_score_cp` as the fixed baseline,
subtracting and replacing only the tuned PST contribution.

Smoke run:

```sh
python3 tools/texel_tune_pst.py \
  analysis/self_play/texel_self_play.csv \
  --limit 5000 \
  --steps 4,2 \
  --passes 1
```

Full first-pass run:

```sh
python3 tools/texel_tune_pst.py \
  analysis/self_play/texel_self_play.csv \
  --steps 4,2,1 \
  --passes 1 \
  > analysis/self_play/pst_tune_self_play.txt
```

The first full run on 293,068 quiet self-play rows produced:

```text
initial_mse=0.16653538
best_mse=0.16554868
delta_mse=0.00098670
```

Validation against `origin/main` at `1+0.01`, 10,000 games:

```text
pst vs baseline: 4243 - 4110 - 1647 [0.507]
Elo difference: +4.6 +/- 6.2
LOS: 92.7%
SPRT: llr 1.05, lbound -2.94, ubound 2.94
```

This did not cross the SPRT accept bound, but it was a non-regressing result
for the first narrow internal-weight tuning slice.

## Player Style Probe

Path: `tools/player_style_probe.mjs`

The player style probe compares Boa's fixed-depth move choices with a reference
player's PGN moves. It reports exact move matches and whether Boa or the
reference move leaves fewer immediate legal replies for the opponent.

This is a style diagnostic, not a strength test. Use it to check whether a
change moves Boa toward the intended restriction personality, then use
cutechess or Match Manager to measure strength.

Default Karpov run:

```sh
node tools/player_style_probe.mjs --depth 4 --positions 80 --stride 19
```

Petrosian run:

```sh
node tools/player_style_probe.mjs \
  --zip games/Petrosian.zip \
  --player petrosian \
  --label Petrosian \
  --depth 4
```

Plain PGN run:

```sh
node tools/player_style_probe.mjs \
  --pgn /path/to/games.pgn \
  --player "karpov|petrosian" \
  --label karpov_petrossian
```

Useful options:

- `--engine FILE`: UCI engine path. Default `target/release/boa`.
- `--zip FILE`: zip archive containing PGN games. Default `games/Karpov.zip`.
- `--member NAME`: PGN member inside the zip. Defaults to first `.pgn` member.
- `--pgn FILE`: read a plain PGN file instead of a zip archive.
- `--player REGEX`: case-insensitive reference player name regex.
- `--label TEXT`: display name used in output.
- `--depth N`: fixed UCI search depth.
- `--positions N`: maximum sampled positions.
- `--stride N`: keep every Nth eligible move.
- `--min-ply N` and `--max-ply N`: restrict sampled game phase.
- `--samples N`: disagreement samples to print.
- `--progress N`: progress interval. Use `0` to suppress progress.

## Openings

Path: `tools/openings.epd`

This is the shared opening suite for Match Manager and direct cutechess runs.
Use it for engine comparisons unless the experiment specifically tests an
opening book or a narrow opening family.

Recommended cutechess opening arguments:

```sh
-openings file=tools/openings.epd format=epd order=random policy=round
```

Keep opening selection fixed across candidate and baseline runs.

## Release Workflow

Path: `.github/workflows/release.yml`

Windows releases are published automatically when a version tag is pushed:

```sh
git tag v0.1.1
git push origin v0.1.1
```

The workflow runs on `windows-latest`, installs stable Rust, runs
`cargo test --locked`, builds `x86_64-pc-windows-msvc`, and uploads:

- `boa-<tag>-windows-x86_64.exe`
- `boa-<tag>-windows-x86_64.zip`
- `SHA256SUMS.txt`

## Choosing The Right Tool

- Need a human approval run or PGN replay: use [match-manager](https://github.com/user/match-manager).
- Need a repeatable agent-run match: use direct `fastchess` from match-manager.
- Need to test whether an existing UCI feature matters: use the ablation runner in match-manager.
- Need to inspect style, not strength: use `player_style_probe.mjs`.
- Need to compare engine strength for a PR: use match-manager snapshots plus
  direct cutechess or match-manager.
