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

## Match Manager

The remaining tool workflow is the terminal Match Manager:

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

## Repository Layout

- `src/`: engine source.
- `games/`: archived reference games.
- `tools/match_manager/`: terminal match workflow and saved match state.
- `tools/openings.epd`: opening suite used by Match Manager.
