# Boa Tools

This directory contains the local tooling used to test Boa, run engine matches,
and inspect whether changes fit the engine's restriction-first style.

## Requirements

- `cargo build --release` must produce `target/release/boa` before tools can run
  the current engine.
- `unzip` is required for tools that read zipped PGN archives.
- Match Manager dependencies are installed with:

```sh
cd tools/match_manager
npm install
```

Generated match state, analysis datasets, and build output should stay local. Do not commit
`target/`, `tools/match_manager/dist/`, `tools/match_manager/engines/`, or
`tools/match_manager/matches/`, or `analysis/`.

## Release Workflow

Path: `.github/workflows/release.yml`

Windows releases are published automatically when a version tag is pushed:

```sh
git tag v0.1.1
git push origin v0.1.1
```

The workflow runs on `windows-latest`, installs the stable Rust toolchain,
runs `cargo test --locked`, builds `x86_64-pc-windows-msvc`, and uploads:

- `boa-<tag>-windows-x86_64.exe`
- `boa-<tag>-windows-x86_64.zip`
- `SHA256SUMS.txt`

## Match Manager

Path: `tools/match_manager/`

The Match Manager is the main terminal UI for approval matches and
non-regression testing. It can snapshot Boa binaries, import existing binaries,
configure cutechess matches, monitor Elo/LOS/SPRT progress, and browse PGNs.

Build and run it with:

```sh
cd tools/match_manager
npm run build
./match-manager
```

For source-mode development:

```sh
cd tools/match_manager
npm run check
npm run dev
```

It uses `tools/cutechess-cli`, `tools/openings.epd`, and
`target/release/boa`. If the local cutechess binary is missing, it will try to
find `cutechess-cli` on `PATH`.

## cutechess-cli

Path: `tools/cutechess-cli`

This binary runs automated engine-vs-engine games. Prefer Match Manager for
interactive approval runs, but direct cutechess commands are useful for quick
scripted checks.

The `tools/cutechess/` directory is the local cutechess source/build checkout
used to produce the binary. Most engine experiments should use the checked-in
`tools/cutechess-cli` wrapper/binary and leave the source checkout alone unless
the match runner itself is being upgraded.

Example non-regression shape:

```sh
tools/cutechess-cli \
  -engine cmd=target/release/boa proto=uci name=candidate option.Hash=64 \
  -engine cmd=/path/to/baseline/boa proto=uci name=baseline option.Hash=64 \
  -each proto=uci tc=5+0.05 \
  -games 2 -rounds 50 -repeat \
  -concurrency 8 \
  -openings file=tools/openings.epd format=epd order=random policy=round \
  -recover -maxmoves 200 \
  -draw movenumber=40 movecount=8 score=10 \
  -resign movecount=5 score=700 twosided=true
```

For feature and optimization work, use either an SPRT run or a non-regression
match and record the time control, openings, game count, and W-D-L result.

## Ablation Runner

Path: `tools/match_manager/src/ablation.ts`

The ablation runner reuses Match Manager's snapshot and cutechess machinery to
test whether individual Boa eval/search terms help. It runs one match per
ablation with the same snapshot on both sides; the candidate side gets one UCI
option changed, such as `Eval Freedom Scale=0`.

First snapshot the current engine in Match Manager, then run:

```sh
cd tools/match_manager
npm run ablate -- --engine baseline_main_boa --games 400 --tc 5+0.05
```

Useful options:

- `--list`: show available ablations.
- `--only no_eval_freedom,no_eval_coordination`: run a subset.
- `--sprt`: enable cutechess SPRT with `elo0=0`, `elo1=5`.

Interpretation: the reported Elo is from the ablated candidate's perspective.
If `no_eval_freedom` loses clearly, the freedom term is useful. If it wins
clearly, the term is harmful or overweighted. If it is within the error bar,
the result is unclear and needs more games or a scale test.

## Openings

Path: `tools/openings.epd`

This is the opening suite used by Match Manager and the direct cutechess command
above. Keep opening selection constant when comparing two engine versions so
the match stays fair and reproducible.

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

## Player Style Probe

Path: `tools/player_style_probe.mjs`

The player style probe compares Boa's fixed-depth best moves against a reference
player's moves from PGN games. It reports:

- exact reference move matches
- how often Boa leaves fewer immediate legal replies for the opponent
- how often the reference move leaves fewer replies
- sample disagreements for inspection

This is a style diagnostic, not a strength test. Use it to check whether a
change pushes Boa toward the intended squeeze/restriction personality, then use
cutechess or Match Manager to check strength.

Default Karpov run:

```sh
node tools/player_style_probe.mjs --depth 4 --positions 80 --stride 19
```

Run against another zipped archive:

```sh
node tools/player_style_probe.mjs \
  --zip games/Petrosian.zip \
  --player petrosian \
  --label Petrosian \
  --depth 4
```

Run against a plain PGN:

```sh
node tools/player_style_probe.mjs \
  --pgn /path/to/games.pgn \
  --player "karpov|petrosian" \
  --label squeeze_masters
```

Useful options:

- `--engine FILE`: UCI engine path. Default: `target/release/boa`.
- `--zip FILE`: zip archive containing PGN games. Default:
  `games/Karpov.zip`.
- `--member NAME`: PGN member inside the zip. If omitted, the first `.pgn`
  member is used.
- `--pgn FILE`: read a plain PGN file instead of a zip archive.
- `--player REGEX`: case-insensitive reference player name regex.
- `--label TEXT`: display name used in output.
- `--depth N`: fixed UCI search depth.
- `--positions N`: maximum sampled positions.
- `--stride N`: keep every Nth eligible move.
- `--min-ply N` / `--max-ply N`: restrict the sampled game phase.
- `--samples N`: disagreement samples to print.
- `--progress N`: progress interval; use `0` to suppress progress.
