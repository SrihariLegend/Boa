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

Generated match state and build output should stay local. Do not commit
`target/`, `tools/match_manager/dist/`, `tools/match_manager/engines/`, or
`tools/match_manager/matches/`.

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

## Openings

Path: `tools/openings.epd`

This is the opening suite used by Match Manager and the direct cutechess command
above. Keep opening selection constant when comparing two engine versions so
the match stays fair and reproducible.

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
