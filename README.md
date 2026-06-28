# Boa

Boa is a UCI chess engine written in Rust. It uses bitboard move generation,
classical tapered evaluation, and alpha-beta search with standard pruning and
move-ordering heuristics.  Late-move reductions are guarded by a learned
logistic criticality model trained on shadow counterfactual probes.

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

## Learned LMR Criticality

Boa's late-move reductions use a learned logistic guard.  Before reducing a
quiet move the engine computes a criticality score from 27 positional and
search-state features.  Moves whose score exceeds the P97 threshold get one
ply of reduction protection.

The model is trained on **shadow counterfactual probes**: during self-play the
engine occasionally searches a reduced move to full depth to see whether the
reduction changed the score.  Those outcomes become training labels.

### Runtime coefficients

At startup the engine loads `criticality.coeffs` from the same directory as
the executable.  If the file is missing or malformed it falls back to the
hardcoded coefficients in `src/search/constants.rs`.

The `.coeffs` format uses plain `key = value` lines.  Everything below the
`---` separator is commented-out version history — the engine never parses it.

### Training pipeline

One script handles everything:

```sh
# Full pipeline: collect self-play games, train model, write .coeffs
python3 tools/train.py all --games 200

# Or step by step:
python3 tools/train.py collect --games 200
python3 tools/train.py train --data analysis/criticality/<run>/raw

# Probe health check:
python3 tools/train.py check --data analysis/criticality/<run>/raw
```

Requirements: Python 3 with numpy and scikit-learn, Node.js, cutechess-cli.

Full documentation: `tools/CRITICALITY_GUIDE.md`.

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
- `tools/` — training pipeline, game runner, opening book, docs.
- `criticality.coeffs` — current LMR criticality coefficients (loaded at runtime).
- `EXPERIMENTS.md` — scratchpad of tried engine ideas, results, and rejected code paths.

## Tooling Docs

- `tools/CRITICALITY_GUIDE.md` — how to use the training pipeline, add/remove features.
- `tools/README.md` — match-running and tool overview.
- `tools/AGENT_GUIDE.md` — non-interactive workflows and reporting rules for coding agents.
