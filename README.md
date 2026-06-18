# Boa

Boa is a UCI chess engine written in Rust. Its evaluation and search are tuned toward restriction, prophylaxis, and squeeze-style positions: reduce the opponent's useful moves, improve steadily, and convert mistakes.

## Build

```sh
cargo build --release
```

The engine binary is written to:

```sh
target/release/boa
```

## Run

Start the engine directly:

```sh
./target/release/boa
```

It speaks UCI, so it can also be loaded by chess GUIs and match runners that support UCI engines.

Useful manual commands:

```text
uci
isready
position startpos
go depth 8
bench 10
perft 5
eval
quit
```

Supported UCI options:

- `Hash`: transposition table size in MB, default `128`, range `1..4096`.
- `Contempt`: draw-avoidance bias in centipawns, default `20`, range `-100..100`.
- Eval scales, all spin options with default `100`, range `0..300`:
  `Eval Material Scale`, `Eval PST Scale`, `Eval Mobility Scale`,
  `Eval Pawn Structure Scale`, `Eval King Safety Scale`,
  `Eval Freedom Scale`, `Eval Trade Down Scale`, `Eval Weak Squares Scale`,
  `Eval Coordination Scale`, and `Eval Advanced Pawns Scale`.
- Search controls: `Search Restriction Ordering` plus
  `Search Restriction Ordering Scale`, `Search Squeeze Extensions`,
  `Search Squeeze Null Move Suppression`, and `Search Squeeze LMR Relief`.

Run `uci` to print the authoritative option list for the current binary.

## Match Manager

The main test workflow is the terminal Match Manager:

```sh
cd tools/match_manager
npm install
npm run build
./match-manager
```

Match Manager can snapshot engine builds, run approval matches through Cute Chess, configure openings and time controls, and browse saved games.

It depends on:

- `tools/cutechess-cli`
- `tools/openings.epd`
- `target/release/boa`

Full documentation:

- Human Match Manager manual: `tools/match_manager/README.md`.
- All tools overview: `tools/README.md`.
- Coding-agent playbook: `tools/AGENT_GUIDE.md`.

## Development

Run the Rust checks:

```sh
cargo test
```

Run Match Manager type checks:

```sh
cd tools/match_manager
npm run check
```

## Releases

GitHub Actions publishes Windows releases automatically from version tags.

```sh
git tag v0.1.1
git push origin v0.1.1
```

The release workflow builds `x86_64-pc-windows-msvc`, runs `cargo test`, and
uploads a raw `boa-<tag>-windows-x86_64.exe`, a zip archive, and
`SHA256SUMS.txt`.

## Repository Layout

- `src/`: engine source.
- `games/`: archived reference games.
- `EXPERIMENTS.md`: scratchpad of tried engine ideas, results, and rejected code paths.
- `tools/match_manager/`: terminal match workflow and saved match state.
- `tools/openings.epd`: opening suite used by Match Manager.
- `tools/player_style_probe.mjs`: reference-player style probe for restriction experiments.

## Documentation For Tooling

Use these docs before running engine experiments:

- `tools/match_manager/README.md`: complete Match Manager usage for humans and
  scripted ablation workflows.
- `tools/README.md`: direct cutechess, ablations, openings, style probe, and
  release-tool overview.
- `tools/AGENT_GUIDE.md`: non-interactive workflows and reporting rules for
  coding agents.
