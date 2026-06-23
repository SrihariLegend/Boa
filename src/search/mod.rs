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
use crate::criticality::{
    should_probe as should_probe_criticality, CriticalityDecisionKind, CriticalityLabelSource,
    CriticalityLogger, CriticalityRecord,
};
use crate::eval::{evaluate, EvalContext};
use crate::movegen::{gen_captures, gen_moves, AttackTables, MoveList};
use crate::syzygy::SyzygyTablebase;
use crate::tt::{score_from_tt, score_to_tt, Bound, TranspositionTable};
use crate::types::*;

mod alpha_beta;
mod bench;
mod constants;
mod context;
mod criticality;
mod move_ordering;
#[cfg(test)]
mod move_ordering_tests;
mod null_move;
mod pruning;
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
pub(in crate::search) use criticality::*;
pub(in crate::search) use move_ordering::*;
pub(in crate::search) use null_move::*;
pub(in crate::search) use pruning::*;
pub(in crate::search) use quiescence::*;
pub(in crate::search) use see::*;
pub(in crate::search) use stats::*;
pub(in crate::search) use tt_cutoff::*;
pub(in crate::search) use types::{
    CriticalityRecordInput, FfpInput, LmrInput, LmrReduction, SearchNode,
};

pub use bench::bench;
pub use context::SearchContext;
pub use root::search;
pub use types::{Limits, SearchResult};
