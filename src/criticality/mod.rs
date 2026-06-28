// ============================================================
// criticality — LMR criticality data pipeline (probe I/O and logging)
//
// This module collects training data: it logs shadow counterfactual
// probes and observed-research probes during search.  The actual
// model inference (scoring a move's criticality) lives in
// `src/search/pruning/lmr.rs`.
// ============================================================

use crate::types::{move_name, piece_type, Color, Move, Piece, PIECE_NONE};
use std::fs::{create_dir_all, metadata, read_dir, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;

mod logger;
mod record;
mod sampling;
#[cfg(test)]
mod tests;

pub use logger::*;
pub use record::*;
pub use sampling::*;
