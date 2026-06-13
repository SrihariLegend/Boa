// karpov — a Karpov-style chess engine
// Architecture: Bitboard move generation → Alpha-Beta search with style-aware pruning → NNUE-ready evaluation
//
// Style philosophy: Hunt for positions where opponent freedom approaches zero.
// Pattern: Restriction → Prophylaxis → Improvement → Near-zugzwang → Error → Conversion

mod board;
mod movegen;
mod search;
mod eval;
mod uci;
mod tt;
mod types;

fn main() {
    uci::run();
}
