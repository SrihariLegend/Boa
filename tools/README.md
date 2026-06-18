# Boa Tools

This directory contains the local tooling used to test Boa, run engine matches,
inspect restriction-style behavior, and manage release support files.

## Tool Map

- `match_manager/`: terminal UI and scripted match workflow.
- `match_manager/src/ablation.ts`: non-interactive ablation and scale runner.
- `cutechess-cli`: local cutechess binary or wrapper used by Match Manager.
- `openings.epd`: opening suite for fair engine comparisons.
- `player_style_probe.mjs`: fixed-depth style diagnostic against reference PGNs.
- `../games/*.zip`: zipped Karpov, Petrosian, and Keres PGN archives.
- `AGENT_GUIDE.md`: concise playbook for coding agents using these tools.

Generated files that should stay local:

- `target/`
- `tools/match_manager/dist/`
- `tools/match_manager/engines/`
- `tools/match_manager/matches/`

## Requirements

Build the current engine before running tools that execute Boa:

```sh
cargo build --release
```

Install Match Manager dependencies once:

```sh
cd tools/match_manager
npm install
```

Other local dependencies:

- `unzip`: required by PGN tools that read zipped archives.
- `tools/cutechess-cli` or `cutechess-cli` on `PATH`: required for matches.
- `stockfish` on `PATH` or `/usr/games/stockfish`: optional Stockfish matches.

## Match Manager

Path: `tools/match_manager/`

The Match Manager is the main human workflow for engine approval matches and
PGN review. It can snapshot Boa binaries, import existing binaries, configure
cutechess matches, monitor Elo/LOS/SPRT progress, stop or delete matches, and
replay PGNs.

Build and run:

```sh
cd tools/match_manager
npm run build
./match-manager
```

Development mode:

```sh
cd tools/match_manager
npm run check
npm run dev
```

Full manual:

```text
tools/match_manager/README.md
```

## Direct cutechess-cli

Path: `tools/cutechess-cli`

Use direct cutechess for scripted non-regression checks when the Match Manager
UI is not appropriate. Keep both engines on the same hash, openings, time
control, adjudication, and concurrency.

Example candidate vs saved baseline:

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

Example SPRT shape:

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

For PRs, record the command shape, time control, openings, game count, W-D-L,
Elo/error or SPRT result, and compared snapshots or commits.

## Ablation Runner

Path: `tools/match_manager/src/ablation.ts`

The ablation runner tests individual UCI-controlled features using the same
snapshot as both engines. The candidate side receives one option override.

List available ablations:

```sh
cd tools/match_manager
npm run ablate -- --list
npm run ablate -- --suite scale --list
```

Run a focused ablation:

```sh
cd tools/match_manager
npm run ablate -- --engine main_baseline --only no_eval_freedom --games 400 --tc 5+0.05
```

Run a scale suite:

```sh
cd tools/match_manager
npm run ablate -- --engine main_baseline --suite scale --games 400 --tc 5+0.05
```

Run with SPRT:

```sh
cd tools/match_manager
npm run ablate -- \
  --engine main_baseline \
  --only no_eval_freedom \
  --games 10000 \
  --tc 1+0.01 \
  --sprt \
  --sprt-elo0 0 \
  --sprt-elo1 5
```

Interpretation: the reported Elo is from the ablated candidate's perspective.
If `no_eval_freedom` loses clearly, the freedom term is useful. If it wins
clearly, the term is harmful or overweighted. If it is inside the error bar,
the result is unclear.

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
  --label squeeze_masters
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

- Need a human approval run or PGN replay: use Match Manager.
- Need a repeatable agent-run match: use direct `tools/cutechess-cli`.
- Need to test whether an existing UCI feature matters: use `npm run ablate`.
- Need to inspect style, not strength: use `player_style_probe.mjs`.
- Need to compare engine strength for a PR: use Match Manager snapshots plus
  direct cutechess or Match Manager.
