// ============================================================
// search.rs - Alpha-beta search with pragmatic pruning policy
//
// Core algorithm: PVS (Principal Variation Search) with iterative deepening.
//
// Search modifications:
//   1. PVS with iterative deepening, aspiration windows, and TT cutoffs
//   2. Null-move pruning, futility pruning, LMR, and quiescence search
//   3. SEE-guided capture ordering and optional losing-capture pruning
//   4. Lazy SMP root search when multiple threads are requested
// ============================================================

use crate::board::{Board, Zobrist};
use crate::config::EngineOptions;
use crate::eval::{evaluate, EvalContext};
use crate::movegen::{gen_captures, gen_moves, AttackTables, MoveList};
use crate::syzygy::SyzygyTablebase;
use crate::tt::{score_from_tt, score_to_tt, Bound, TranspositionTable};
use crate::types::*;

mod alpha_beta;
mod bench;
mod constants;
mod context;
mod correction;
mod move_ordering;
#[cfg(test)]
mod move_ordering_tests;
mod null_move;
pub mod pruning;
#[cfg(test)]
mod pruning_tests;
mod quiescence;
#[cfg(test)]
mod quiescence_tests;
mod root;
#[cfg(test)]
mod root_tests;
mod see;
#[cfg(test)]
mod see_tests;
mod stats;
#[cfg(test)]
mod test_utils;
mod tt_cutoff;
mod types;

pub(in crate::search) use alpha_beta::*;
pub(in crate::search) use constants::*;
pub(in crate::search) use context::now_ms;
pub(in crate::search) use correction::*;
pub(in crate::search) use move_ordering::*;
pub(in crate::search) use null_move::*;
pub(in crate::search) use pruning::*;
pub(in crate::search) use quiescence::*;
pub(in crate::search) use see::*;
pub(in crate::search) use stats::*;
pub(in crate::search) use tt_cutoff::*;
pub(in crate::search) use types::{
    LmrInput, LmrReduction, SearchNode,
};

pub use bench::bench;
pub use context::SearchContext;
pub use root::search;
pub use types::{FfpInput, Limits, SearchResult};

/// Quick fixed-depth search for diagnostics. Returns the score from the
/// perspective of the side to move. Not for gameplay — uses caller-provided
/// TT and attack tables (reuse across calls to avoid allocation).
pub fn quick_search(
    board: &mut Board,
    options: &EngineOptions,
    depth: i32,
    atk: &crate::movegen::AttackTables,
    z: &crate::board::Zobrist,
    tt: &crate::tt::TranspositionTable,
) -> Score {
    use std::sync::atomic::AtomicBool;

    let stop = AtomicBool::new(false);
    let limits = Limits { max_depth: depth as u32, ..Default::default() };

    let mut ctx = SearchContext::new(
        atk, z, tt, limits, Vec::new(), 0,
        options.clone(), None, &stop, 0, 0,
    );

    let mut pv = Vec::new();
    alpha_beta(
        board,
        &mut ctx,
        SearchNode { alpha: -SCORE_INF, beta: SCORE_INF, depth, ply: 0, is_pv: true },
        &mut pv,
    )
}
