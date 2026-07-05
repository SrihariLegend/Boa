# Boa — Claude Code project guide

Boa is a UCI chess engine written in Rust.  Bitboard move generation,
classical tapered evaluation, and alpha-beta search.

## Token-efficient tool use

These rules are global — every tool call must respect them to keep context
small and the user's cost low.

- **Never stream raw large output into context.**  Assume anything over ~100
  lines is dangerous.  Redirect to a temp file and extract only what matters.
- **Read code surgically.**  Prefer targeted line ranges, function-level
  reads, and `grep` over whole-file reads.  Read the smallest useful scope.
- **Read data statistically.**  For CSV, JSON, PGN, logs: use counts,
  schemas, file sizes, column headers — not row dumps.
- **Cap exploratory searches.**  Always pipe `grep -R` / `find` through
  `head -80` or `grep -m`.  Never let unbounded search results into context.
- **Tests and builds:** use quiet flags (`cargo test -q`), log to temp files,
  extract only pass/fail + error diagnostics.  Show full output only on
  failure and even then cap it.
- **Training, matches, benchmarks:** always log to file and post-process.
  Extract only final metrics, panic/error lines, or small summary tables.
  Never ingest full stdout from long-running workflows.
- **Recovery from verbosity:** if a command emits too much, stop that
  pattern.  Next attempt must redirect to file and extract the minimum
  diagnostic subset.

## Project structure

```
src/              — engine source (main, uci, board, movegen, search, eval, tt)
  search/pruning/ — FFP, RFP, LMR
tools/            — testing pipeline, game runner, opening book
analysis/         — generated data (not committed)
```

## Build, test, and development

```sh
cargo build --release    # optimized binary → target/release/boa
cargo test               # all tests (unit + integration)
cargo check              # fast compile check, no binary
```

Start the engine: `./target/release/boa` — it speaks UCI.

## Key engineering lessons

- **ML approach for pruning did not work.**  A unified ML model across the
  pruning subsystem (FFP, RFP, and LMR criticality) failed to gain Elo or was
  ultimately stripped in favor of classical heuristics. Pruning remains classical
  (simple margin formulas and log-based reductions).  Do not reintroduce learned models
  for these without strong SPRT evidence.
- **Correction History works.** Dynamically learning systematic static evaluation biases
  using non-pawn Zobrist hashes at runtime provides a robust way to fix eval holes.
- **SPRT everything.**  No eval or search change ships without a passing
  SPRT at fast time control (1+0.01 or similar).  Internal test metrics
  (AUC, RMSE, Pearson) are diagnostics — they do not substitute for
  playing-strength validation.

## Probe System

When adding a new module to the engine, you MUST add probe events for it:
1. Define event struct in `src/probe/events.rs`
2. Add variant to `ProbeEvent` enum with its short `typ` code
3. Add `probe!()` or `sample_probe!()` calls in the module's key decision points
4. Add the module's field legend to the `meta_json()` function in `src/probe/mod.rs`

Build with `cargo build --release --features probes` for diagnostic-enabled engine.
Output goes to `logs/boa-probe-<timestamp>.jsonl` — one file per search.
Full spec: `docs/superpowers/specs/2026-06-29-probe-system-design.md`

## Coding and testing conventions

- Use Rust 2021 idioms and `rustfmt` formatting.  Keep modules focused on
  their chess-engine responsibility; prefer descriptive `snake_case`.
- Add tests beside the code under `#[cfg(test)] mod tests`.  Use
  position-driven assertions when possible and include a regression test
  for every bug fix.
- Keep local paths configurable (env vars or CLI flags).  Never commit a
  hard-coded machine-specific absolute path.

## Commit conventions

- Short imperative subjects: `Fix search edge cases`, `Add correction history`.
- Engine behaviour changes: include SPRT result or note that no match was run.
- Tooling changes: include the command shape and output path changes.
- End commit messages with `Co-Authored-By: Claude <noreply@anthropic.com>`.
- PRs must include: summary, commands run, affected behaviour, verification
  (tests passed / match result / SPRT status).

## Do not commit

- `target/`, `analysis/`, `__pycache__/`, `*.pyc`, `*.log`
- Generated binaries (cutechess-cli, etc.)
- Match results, PGNs, training datasets
