# Tooling Guide For Coding Agents

This guide is for coding agents working in Boa. Prefer it when you need a
repeatable, non-interactive workflow. For match management (snapshots, ablations,
terminal UI), see the standalone [match-manager](https://github.com/user/match-manager) repo.

## Ground Rules

- Do not commit generated files from `target/` or `analysis/`.
- Do not start long SPRT or high-game-count matches without explicit user
  approval.
- Prefer scripted commands over interactive UIs.
- If a match result is reported, include command shape, time control, openings,
  game count, W-D-L, Elo/error or SPRT result, and compared builds.

## Standard Validation

Run these before opening a PR that touches engine or tooling code:

```sh
cargo test
cargo build --release
git diff --check
```

For docs-only changes, use at least:

```sh
git diff --check
```

## Snapshot Policy

Engine snapshots are managed by the standalone
[match-manager](https://github.com/user/match-manager) repo. See its README for
snapshot and import workflows.

Agents should not assume a snapshot exists. If no suitable baseline exists, ask
the user to create one.

## Non-Regression Match

Use this for a quick scripted candidate vs baseline check. Replace
`<snapshot_path>` with the actual engine snapshot binary path.

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

Use a shorter run only to catch catastrophic regressions. Do not claim Elo from
tiny samples.

## SPRT Match

Use SPRT for feature or optimization approval when the user asks for a strength
test.

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

Stop and report when cutechess accepts H0 or H1. If the run is interrupted,
report partial W-D-L and that the result is inconclusive.

## Ablations

Ablations are managed by the standalone [match-manager](https://github.com/user/match-manager) repo.
See its README for the `npm run ablate` workflow, `--list`, `--suite scale`,
`--sprt`, and interpretation details.

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
option.Eval Mobility Scale=0
option.Search SEE=false
```

In Match Manager `extraA` and `extraB`, pass options as comma-separated
`name=value` entries:

```text
Eval Mobility Scale=0,Search SEE=false
```

## Reporting Template

Use this shape in final answers and PR bodies:

```text
Validation:
- cargo test
- cargo build --release

Match:
- candidate: <branch/commit/snapshot>
- baseline: <branch/commit/snapshot>
- command: cutechess, tc 5+0.05, openings tools/openings.epd, hash 64, concurrency 8
- result: +W =D -L, N games, Elo X +/- Y, LOS Z%
- SPRT: PASSED/FAILED/running/inconclusive, llr <value>
```

If no match was run, say that explicitly and explain why.
