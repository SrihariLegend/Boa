// ============================================================
// syzygy.rs - Syzygy tablebase integration
// ============================================================

use crate::board::{Board, Zobrist};
use crate::config::SyzygyOptions;
use crate::movegen::{gen_moves, AttackTables};
use crate::types::*;
use shakmaty::{fen::Fen, CastlingMode, Chess};
use shakmaty_syzygy::{AmbiguousWdl, Tablebase, Wdl};
use std::{io, path::Path};

mod constants;
mod legal;
mod paths;
mod probe;
mod tablebase;
#[cfg(test)]
mod tests;
mod types;

use constants::*;
use legal::*;
use paths::*;
use probe::*;

pub use types::{SyzygyRootProbe, SyzygyTablebase};
