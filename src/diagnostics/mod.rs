// ============================================================
// diagnostics.rs - offline feature extraction for engine research
// ============================================================

use crate::board::{Board, Zobrist};
use crate::config::EngineOptions;
use crate::eval::{evaluate_breakdown, side_mobility, EvalContext};
use crate::movegen::{gen_moves, AttackTables};
use crate::types::*;

mod pawn_breaks;
mod redeployment;
mod restriction;
#[cfg(test)]
mod tests;

pub(in crate::diagnostics) use pawn_breaks::*;
pub(in crate::diagnostics) use redeployment::*;
pub(in crate::diagnostics) use restriction::{
    LIBERATING_MOBILITY_GAIN, REDEPLOYMENT_MOBILITY_GAIN,
};

pub use restriction::{extract_restriction_features, RestrictionFeatures};
