// boa — a Boa-style chess engine
// Architecture: Bitboard move generation → Alpha-Beta search with style-aware pruning → NNUE-ready evaluation
//
// Style philosophy: Hunt for positions where opponent freedom approaches zero.
// Pattern: Restriction → Prophylaxis → Improvement → Near-zugzwang → Error → Conversion

mod board;
mod config;
mod diagnostics;
mod eval;
mod movegen;
mod search;
mod tt;
mod types;
mod uci;

fn main() {
    uci::run();
}
