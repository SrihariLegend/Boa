// boa - a UCI chess engine
// Architecture: bitboard move generation, classical evaluation, and alpha-beta search.

mod board;
mod config;
mod criticality;
mod diagnostics;
mod eval;
mod movegen;
mod search;
mod syzygy;
mod tt;
mod types;
mod uci;

fn main() {
    uci::run();
}
