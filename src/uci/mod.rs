// ============================================================
// uci.rs — UCI protocol handler
// ============================================================

use crate::board::{Board, Zobrist};
use crate::config::EngineOptions;
use crate::diagnostics::{extract_restriction_features, RestrictionFeatures};
use crate::movegen::{perft, AttackTables};
use crate::search::{search, Limits, SearchContext};
use crate::syzygy::SyzygyTablebase;
use crate::tt::TranspositionTable;
use crate::types::*;
use std::io::{self, BufRead, Write};

mod go;
mod legal;
mod r#loop;
mod options;
mod perft;
mod position;
mod setoption;

use go::*;
use legal::*;
use options::*;
use perft::*;
use position::*;
use setoption::*;

pub use r#loop::run;
