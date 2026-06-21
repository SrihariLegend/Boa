# Repository Guidelines
NEVER READ LOGS OR BIG ARTIFACTS DIRECTYL!!! THEY CONSUME TOO MUCH TOKENS AND POISION THE CONTEXT!!!

WHEN YOU ABSOULTELY MUST READ LOGS, USE SCPRITS TO FILTER!!! DO NOT EVER RUN CUTECHESS OR ANY MATCH MANGERS!!!
## Project Structure & Module Organization

Boa is a UCI chess engine written in Rust. Core engine code lives in `src/`: `main.rs` starts the UCI loop, `uci.rs` handles protocol commands, `board.rs` and `movegen.rs` model positions and legal moves, `search.rs` contains alpha-beta search, `eval.rs` contains evaluation, and `tt.rs` implements the transposition table. Reference game archives are in `games/`. Tooling lives under `tools/`, including `tools/openings.epd` and the TypeScript terminal Match Manager in `tools/match_manager/src/`.

## Build, Test, and Development Commands

- `cargo build --release`: build the optimized engine at `target/release/boa`.
- `cargo test`: run Rust unit tests embedded in engine modules.
- `./target/release/boa`: start the UCI engine; useful commands include `uci`, `isready`, `go depth 8`, `bench 10`, and `perft 5`.
- `cd tools/match_manager && npm install`: install Match Manager dependencies.
- `cd tools/match_manager && npm run build`: compile TypeScript to `dist/`.
- `cd tools/match_manager && npm run check`: run TypeScript type checks without emitting files.
- `cd tools/match_manager && npm run dev`: run the Match Manager CLI from source.

## Coding Style & Naming Conventions

Use Rust 2021 idioms and `rustfmt` formatting. Keep modules focused on their chess-engine responsibility and prefer descriptive snake_case for functions, variables, and test names. Preserve existing compact bitboard-oriented style where performance matters, and use `#[rustfmt::skip]` only for intentional tables or layouts. TypeScript uses strict ES module settings, React JSX, and camelCase identifiers.

## Testing Guidelines

Add Rust tests beside the code under `#[cfg(test)] mod tests`, especially for evaluation and search edge cases. Use position-driven assertions when possible and include regression tests for bug fixes. Run `cargo test` before submitting engine changes. For Match Manager changes, run `npm run check`; add focused tests only if a test framework is introduced.

## Commit & Pull Request Guidelines

Recent commits use short imperative subjects such as `Fix search edge cases` and merge PRs from focused branches. Keep commits narrow and explain behavioral engine changes in the body when useful. Pull requests should include a concise summary, commands run, affected UCI behavior or tool workflow, and any relevant benchmark, perft, or match results. Include screenshots only for terminal UI changes where visual layout is important.

## Security & Configuration Tips

Do not commit generated outputs such as `target/`, `tools/match_manager/dist/`, or local match state. Match Manager depends on `tools/cutechess-cli`, `tools/openings.epd`, and `target/release/boa`; keep local paths configurable and avoid hard-coding machine-specific locations.
