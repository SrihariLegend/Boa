// boa - a UCI chess engine
// Architecture: bitboard move generation, classical evaluation, and alpha-beta search.

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
