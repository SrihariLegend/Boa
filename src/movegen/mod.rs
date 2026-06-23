// ============================================================
// movegen.rs — Bitboard move generation with magic bitboards
// ============================================================
//
// Uses magic bitboards for bishop and rook sliding attacks.
// Magic numbers are found at runtime via brute-force search during init().

use crate::board::Board;
use crate::types::*;

mod attacks;
mod generation;
mod movelist;
mod perft;

pub use attacks::*;
pub use generation::*;
pub use movelist::*;
pub use perft::*;
