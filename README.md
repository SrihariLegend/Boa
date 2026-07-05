# Boa

Boa is a UCI chess engine written in Rust. It uses bitboard move generation,
classical tapered evaluation, and alpha-beta search with standard pruning and
move-ordering heuristics.

## Build

```sh
cargo build --release
```

The engine binary is written to `target/release/boa`.

## Run

Start the engine directly:

```sh
./target/release/boa
```

It speaks UCI, so it can also be loaded by chess GUIs and match runners that
support UCI engines.

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
- `Threads`: search worker count, default `1`, range `1..64`.
- `Contempt`: draw-avoidance bias in centipawns, default `0`, range `-100..100`.
- Eval scales, all spin options with default `100`, range `0..300`:
  `Eval Material Scale`, `Eval PST Scale`, `Eval Mobility Scale`,
  `Eval Pawn Structure Scale`, and `Eval King Safety Scale`.
- Search controls: `Search Lazy SMP`, `Search SEE`,
  `Search SEE QSearch Pruning`, and `Search SEE Capture Ordering`.

Run `uci` to print the authoritative option list for the current binary.

## Development

```sh
cargo test
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

- `src/` — engine source.
- `games/` — archived reference games.
- `tools/` — testing pipeline, game runner, opening book, docs.
- `docs/` — design documents, experiments and specifications.

## Tooling Docs

- `tools/README.md` — match-running and tool overview.
- `tools/AGENT_GUIDE.md` — non-interactive workflows and reporting rules for coding agents.
