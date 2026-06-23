// ============================================================
// tt.rs - Transposition table
// ============================================================

use crate::types::{Move, Score, MAX_PLY, SCORE_MATE};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

mod atomic_slot;
mod entry;
mod packing;
mod score;
mod table;
#[cfg(test)]
mod tests;

use atomic_slot::*;
use packing::*;

pub use entry::{Bound, TtEntry};
pub use score::{score_from_tt, score_to_tt};
pub use table::TranspositionTable;
