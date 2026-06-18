# Tooling Guide For Coding Agents

This guide is for coding agents working in Boa. Prefer it when you need a
repeatable, non-interactive workflow. Human-oriented details live in
`tools/match_manager/README.md`.

## Ground Rules

- Do not commit generated files from `target/`, `tools/match_manager/dist/`,
  `tools/match_manager/engines/`, or `tools/match_manager/matches/`.
- Do not delete snapshots or match directories unless the user explicitly asks.
- Do not start long SPRT or high-game-count matches without explicit user
  approval.
- Prefer scripted commands over the interactive Match Manager UI.
- If a match result is reported, include command shape, time control, openings,
  game count, W-D-L, Elo/error or SPRT result, and compared builds.

## Standard Validation

Run these before opening a PR that touches engine or tooling code:

```sh
cargo test
cargo build --release
cd tools/match_manager && npm run check
git diff --check
```

For docs-only changes, use at least:

```sh
git diff --check
```

## Snapshot Policy

Snapshots live under `tools/match_manager/engines/` and are local artifacts.
They are useful because they freeze a baseline binary while source code changes.

Agents should not assume a snapshot exists. Check first:

```sh
find tools/match_manager/engines -maxdepth 2 -type f -name meta.json -print
```

If no suitable baseline exists, ask the user to create one or ask for approval
before creating local snapshot artifacts. The human UI path is:

```sh
cd tools/match_manager
./match-manager
```

Then use `Engine Library` -> `Snapshot Current Build`.

## Non-Regression Match

Use this for a quick scripted candidate vs baseline check. Replace
`main_baseline` with an actual snapshot name.

```sh
tools/cutechess-cli \
  -engine cmd=target/release/boa proto=uci name=candidate option.Hash=64 \
  -engine cmd=tools/match_manager/engines/main_baseline/boa proto=uci name=baseline option.Hash=64 \
  -each proto=uci tc=5+0.05 \
  -games 2 -rounds 200 -repeat \
  -concurrency 8 \
  -openings file=tools/openings.epd format=epd order=random policy=round \
  -recover -maxmoves 200 \
  -draw movenumber=40 movecount=8 score=10 \
  -resign movecount=5 score=700 twosided=true
```

Use a shorter run only to catch catastrophic regressions. Do not claim Elo from
tiny samples.

## SPRT Match

Use SPRT for feature or optimization approval when the user asks for a strength
test.

```sh
tools/cutechess-cli \
  -engine cmd=target/release/boa proto=uci name=candidate option.Hash=64 \
  -engine cmd=tools/match_manager/engines/main_baseline/boa proto=uci name=baseline option.Hash=64 \
  -each proto=uci tc=1+0.01 \
  -games 2 -rounds 5000 -repeat \
  -concurrency 8 \
  -openings file=tools/openings.epd format=epd order=random policy=round \
  -recover -maxmoves 200 \
  -draw movenumber=40 movecount=8 score=10 \
  -resign movecount=5 score=700 twosided=true \
  -sprt elo0=0 elo1=5 alpha=0.05 beta=0.05
```

Stop and report when cutechess accepts H0 or H1. If the run is interrupted,
report partial W-D-L and that the result is inconclusive.

## Ablations

Use ablations when testing UCI-scaled features on the same engine snapshot.

List available ablations:

```sh
cd tools/match_manager
npm run ablate -- --list
npm run ablate -- --suite scale --list
```

Focused ablation:

```sh
cd tools/match_manager
npm run ablate -- --engine main_baseline --only no_eval_freedom --games 400 --tc 5+0.05
```

Scale sweep:

```sh
cd tools/match_manager
npm run ablate -- --engine main_baseline --suite scale --games 400 --tc 5+0.05
```

Interpretation is from the ablated candidate's perspective. For example,
`no_eval_freedom` losing means the freedom term helped the baseline.

## Style Probe

Use this only for style diagnostics. It is not a strength test.

```sh
node tools/player_style_probe.mjs --depth 4 --positions 80 --stride 19
```

For Petrosian:

```sh
node tools/player_style_probe.mjs \
  --zip games/Petrosian.zip \
  --player petrosian \
  --label Petrosian \
  --depth 4
```

Report exact move-match rate, restriction comparison rate, depth, sampled
positions, player/archive, and any notable disagreement samples.

## UCI Option Checks

List engine UCI options by running:

```sh
target/release/boa
uci
```

In cutechess, pass options as:

```text
option.Hash=64
option.Eval Freedom Scale=0
option.Search Squeeze Extensions=false
```

In Match Manager `extraA` and `extraB`, pass options as comma-separated
`name=value` entries:

```text
Eval Freedom Scale=0,Search Squeeze Extensions=false
```

## Reporting Template

Use this shape in final answers and PR bodies:

```text
Validation:
- cargo test
- cargo build --release
- cd tools/match_manager && npm run check

Match:
- candidate: <branch/commit/snapshot>
- baseline: <branch/commit/snapshot>
- command: cutechess, tc 5+0.05, openings tools/openings.epd, hash 64, concurrency 8
- result: +W =D -L, N games, Elo X +/- Y, LOS Z%
- SPRT: PASSED/FAILED/running/inconclusive, llr <value>
```

If no match was run, say that explicitly and explain why.
