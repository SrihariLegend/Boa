# Boa Match Manager

The Match Manager is Boa's terminal workflow for engine snapshots, approval
matches, non-regression matches, Stockfish checks, ablations, and PGN review.
It wraps `cutechess-cli`, keeps match state on disk, and gives humans an
interactive terminal UI while also exposing scripted workflows that coding
agents can use safely.

## Quick Start

From the repository root:

```sh
cargo build --release
cd tools/match_manager
npm install
npm run build
./match-manager
```

For source-mode development:

```sh
cd tools/match_manager
npm run check
npm run dev
```

The UI uses the current terminal. If your terminal has issues with the
alternate screen, run:

```sh
cd tools/match_manager
npm run dev -- --no-alt-screen
```

## Requirements

- Rust toolchain: `cargo build --release` must produce `target/release/boa`.
- Node dependencies: install once with `cd tools/match_manager && npm install`.
- Match runner: `tools/cutechess-cli` if present, otherwise `cutechess-cli` on
  `PATH`.
- Opening suite: `tools/openings.epd`.
- Optional Stockfish checks: `stockfish` on `PATH`, otherwise
  `/usr/games/stockfish`.

## Stored Files

Match Manager persists local state under `tools/match_manager/`:

- `engines/<snapshot>/boa`: copied engine binary for a saved snapshot.
- `engines/<snapshot>/meta.json`: snapshot name, note, creation time, source.
- `matches/<id>/config.json`: match configuration.
- `matches/<id>/status.json`: status and live parsed results.
- `matches/<id>/cutechess.log`: exact command and cutechess output.
- `matches/<id>/games.pgn`: games for replay and inspection.

Do not commit `engines/`, `matches/`, or `dist/`.

## Human UI Workflow

Start the UI:

```sh
cd tools/match_manager
./match-manager
```

Main screens:

- `Engine Library`: snapshot the current release build, import an existing Boa
  binary, or delete old snapshots.
- `New Match`: configure and start a cutechess match.
- `Matches`: monitor, stop, delete, and replay saved matches.

Navigation:

- Arrow keys move the selection.
- Enter opens or confirms the selected action.
- `b`, `q`, or Escape generally backs out of a screen.
- On form screens, Enter edits or accepts the selected field.
- On match detail screens, `s` stops a running match and `d` deletes a stopped
  or finished match after confirmation.

## Snapshot Workflow

Use snapshots whenever comparing engine versions. A snapshot freezes the binary
so later source edits do not change the baseline.

1. Build and snapshot the current baseline before editing:

   ```sh
   cargo build --release
   cd tools/match_manager
   ./match-manager
   ```

2. In `Engine Library`, choose `Snapshot Current Build`.
3. Use a clear name such as `main_20260618` or `phase0_baseline`.
4. Add a note with the branch, commit, or experiment.
5. After implementing a candidate, snapshot it with another clear name.

Imported binaries work the same way, but copy an existing binary path instead
of running `cargo build --release`.

## New Match Settings

The `New Match` form creates a cutechess match between two engine specs.

Engine fields:

- `engineA` and `engineB`: either saved snapshots or Stockfish.
- `eloA` and `eloB`: used only when that side is Stockfish.
- `extraA` and `extraB`: comma-separated UCI option overrides for that side.

Common settings:

- `games`: total requested games. Internally Match Manager uses `-games 2` and
  enough rounds to reach this number, with colors repeated.
- `tc`: cutechess time control, for example `5+0.05`, `1+0.01`, or `40/60`.
- `concurrency`: parallel games. Default is CPU count minus four, capped at 20.
- `hash`: Boa hash size in MB for snapshot engines.
- `openings`: use `tools/openings.epd` with random round order.
- `adjudication`: enable draw and resign adjudication.
- `sprt`: enable cutechess SPRT.
- `sprt_elo0` and `sprt_elo1`: lower and upper SPRT hypotheses.

Extra UCI options are comma-separated:

```text
Eval Mobility Scale=0,Search SEE=false
```

The options are passed as cutechess `option.<name>=<value>` arguments. Use exact
UCI option names as printed by:

```sh
target/release/boa
uci
```

## Result Interpretation

Match Manager reports results from engine A's perspective:

- `Score`: `+wins =draws -losses`.
- `Games`: parsed completed game count.
- `Elo`: estimated Elo difference for engine A vs engine B.
- `LOS`: likelihood of superiority for engine A.
- `SPRT`: running log-likelihood ratio and final pass/fail when available.

For feature work:

- Candidate vs baseline should use identical openings, hash, time control, and
  concurrency.
- A short non-regression run can catch severe losses quickly.
- A real strength claim should use SPRT or enough games that the Elo error bar
  is meaningful.
- Record W-D-L, Elo, error, LOS or SPRT, time control, openings, and commit or
  snapshot names in the PR.

## Scripted Ablation Runner

The ablation runner reuses Match Manager's snapshot and cutechess machinery.
It is the preferred non-interactive path for coding agents and batch tests.

List built-in ablations:

```sh
cd tools/match_manager
npm run ablate -- --list
npm run ablate -- --suite scale --list
```

Run one ablation:

```sh
cd tools/match_manager
npm run ablate -- --engine main_20260618 --only no_eval_freedom --games 400 --tc 5+0.05
```

Run scale sweeps:

```sh
cd tools/match_manager
npm run ablate -- --engine main_20260618 --suite scale --games 400 --tc 5+0.05
```

Run with SPRT:

```sh
cd tools/match_manager
npm run ablate -- \
  --engine main_20260618 \
  --only no_eval_freedom \
  --games 10000 \
  --tc 1+0.01 \
  --sprt \
  --sprt-elo0 0 \
  --sprt-elo1 5
```

Important interpretation detail: ablation results are from the ablated
candidate's perspective. If `no_eval_freedom` is strongly negative, the removed
term is useful. If it is strongly positive, the term is harmful or overweighted.

The runner writes match directories plus a suite summary JSON under
`tools/match_manager/matches/`.

## Coding Agent Playbook

Coding agents should prefer scripted, reproducible commands over the
interactive UI.

Use this workflow for a normal engine PR:

1. Confirm the tree is clean and build the candidate:

   ```sh
   git status --short --branch
   cargo test
   cargo build --release
   cd tools/match_manager && npm run check
   ```

2. If a baseline snapshot does not already exist, ask the human to create one
   through the UI or create it only when explicitly authorized. Snapshot
   creation writes local binaries under `tools/match_manager/engines/`.

3. For scripted strength checks, use direct cutechess or `npm run ablate`.
   Avoid launching `./match-manager` from an automation context unless the
   human explicitly wants the interactive UI.

4. Do not delete `matches/` or `engines/` unless explicitly asked.

5. Report exact commands and result lines. Do not summarize a match as
   "passed" without W-D-L, game count, time control, openings, and SPRT/LOS.

Good agent command patterns:

```sh
cd tools/match_manager
npm run ablate -- --engine main_20260618 --only no_eval_freedom --games 400 --tc 5+0.05
```

```sh
tools/cutechess-cli \
  -engine cmd=target/release/boa proto=uci name=candidate option.Hash=64 \
  -engine cmd=tools/match_manager/engines/main_20260618/boa proto=uci name=baseline option.Hash=64 \
  -each proto=uci tc=5+0.05 \
  -games 2 -rounds 200 -repeat \
  -concurrency 8 \
  -openings file=tools/openings.epd format=epd order=random policy=round \
  -recover -maxmoves 200 \
  -draw movenumber=40 movecount=8 score=10 \
  -resign movecount=5 score=700 twosided=true
```

## Troubleshooting

- `No snapshot named ...`: create or import that engine in `Engine Library`, or
  check the sanitized name under `tools/match_manager/engines/`.
- `cutechess exited with code ...`: read the first command and later error
  output in `matches/<id>/cutechess.log`.
- Stockfish match fails: install `stockfish` or confirm `/usr/games/stockfish`
  exists.
- No PGNs appear yet: wait for finished games. PGNs are written to
  `matches/<id>/games.pgn`.
- UI layout looks broken: try `npm run dev -- --no-alt-screen`.
- Engine options are ignored: check exact UCI option spelling and values by
  running `target/release/boa` then `uci`.
