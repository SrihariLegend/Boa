// ============================================================
// eval.rs - Classical tapered evaluation
//
// Score is always from the perspective of the side to move (negamax).
//
// Structure:
//   1. Piece-square tables (midgame + endgame, tapered)
//   2. Pawn structure (passed, isolated, doubled, chains)
//   3. Mobility
//   4. Piece activity (outposts, rooks on open files, bishop pair)
//   5. King safety
//   6. Passed pawn advancement safety (clear path + king support)
// ============================================================

use crate::board::Board;
use crate::config::{scale_score_pair, EngineOptions};
use crate::movegen::{pawn_attacks_black, pawn_attacks_white, AttackTables};
use crate::types::*;

mod constants;
mod king;
mod material;
mod mobility;
mod mobility_tables;
mod pawns;
mod pst;
#[cfg(test)]
mod tests;
mod types;

pub(in crate::eval) use constants::*;
pub(in crate::eval) use king::*;
pub(in crate::eval) use material::*;
pub(in crate::eval) use mobility::*;
pub(in crate::eval) use mobility_tables::*;
pub(in crate::eval) use pawns::*;
pub(in crate::eval) use pst::*;

pub(crate) use mobility::side_mobility;
pub(crate) use pawns::PawnEvalCache;
pub use types::{evaluate, evaluate_breakdown, EvalContext};
