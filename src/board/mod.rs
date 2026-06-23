// ============================================================
// board.rs — Board representation, FEN parsing, make/unmake move
// ============================================================

use crate::types::*;

mod castling;
mod fen_helpers;
mod impls;
mod state;
mod zobrist;

use castling::*;
use fen_helpers::*;

pub use state::{Board, UndoInfo};
pub use zobrist::Zobrist;
