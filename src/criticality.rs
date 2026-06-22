// ============================================================
// criticality.rs - LMR criticality data collection
// ============================================================

use crate::types::{move_name, piece_type, Color, Move, Piece, PIECE_NONE};
use std::fs::{create_dir_all, metadata, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;

pub const CRITICALITY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CriticalityEffect {
    None,
    Alpha,
    Cutoff,
}

impl CriticalityEffect {
    pub fn from_score(score: i32, alpha: i32, beta: i32) -> Self {
        if score >= beta {
            return CriticalityEffect::Cutoff;
        }
        if score > alpha {
            return CriticalityEffect::Alpha;
        }
        CriticalityEffect::None
    }

    fn as_str(self) -> &'static str {
        match self {
            CriticalityEffect::None => "none",
            CriticalityEffect::Alpha => "alpha",
            CriticalityEffect::Cutoff => "cutoff",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CriticalityLabelSource {
    None,
    ObservedResearch,
    CounterfactualProbe,
}

impl CriticalityLabelSource {
    fn as_str(self) -> &'static str {
        match self {
            CriticalityLabelSource::None => "none",
            CriticalityLabelSource::ObservedResearch => "observed_research",
            CriticalityLabelSource::CounterfactualProbe => "counterfactual_probe",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CriticalityDecisionKind {
    Lmr,
}

impl CriticalityDecisionKind {
    fn as_str(self) -> &'static str {
        match self {
            CriticalityDecisionKind::Lmr => "lmr",
        }
    }
}

#[derive(Clone, Debug)]
pub struct CriticalityRecord {
    pub decision_kind: CriticalityDecisionKind,
    pub pid: u32,
    pub game_id: u64,
    pub search_id: u64,
    pub root_depth: i32,
    pub ply: usize,
    pub node_hash: u64,
    pub side_to_move: Color,
    pub m: Move,
    pub from: u8,
    pub to: u8,
    pub piece: Piece,
    pub depth: i32,
    pub move_index: usize,
    pub base_reduction: i32,
    pub final_reduction: i32,
    pub new_depth: i32,
    pub history_score: i32,
    pub static_eval: i32,
    pub prev_static_eval: Option<i32>,
    pub alpha: i32,
    pub beta: i32,
    pub is_pv: bool,
    pub is_cut_node: bool,
    pub improving: bool,
    pub is_killer: bool,
    pub is_counter: bool,
    pub tt_move_agreement: bool,
    pub label_source: CriticalityLabelSource,
    pub reduced_score: Option<i32>,
    pub full_score: Option<i32>,
}

impl CriticalityRecord {
    pub fn header() -> &'static str {
        concat!(
            "schema_version,decision_kind,pid,game_id,search_id,root_depth,ply,node_hash,side_to_move,move_uci,",
            "from,to,piece_type,depth,move_index,base_reduction,final_reduction,new_depth,",
            "history_score,static_eval,has_prev_static_eval,prev_static_eval,static_eval_delta,",
            "alpha,beta,is_pv,is_cut_node,improving,is_killer,is_counter,tt_move_agreement,",
            "label_source,reduced_score,full_score,score_delta_cp,reduced_effect,full_effect,bound_changed",
        )
    }

    pub fn to_csv_row(&self) -> String {
        let (has_prev, prev, delta) = match self.prev_static_eval {
            Some(prev) => (
                "1".to_string(),
                prev.to_string(),
                (self.static_eval - prev).to_string(),
            ),
            None => ("0".to_string(), String::new(), String::new()),
        };
        let reduced = self
            .reduced_score
            .map_or_else(String::new, |score| score.to_string());
        let full = self
            .full_score
            .map_or_else(String::new, |score| score.to_string());
        let score_delta = match (self.reduced_score, self.full_score) {
            (Some(reduced), Some(full)) => (full - reduced).abs().to_string(),
            _ => String::new(),
        };
        let reduced_effect = self.reduced_score.map_or(CriticalityEffect::None, |score| {
            CriticalityEffect::from_score(score, self.alpha, self.beta)
        });
        let full_effect = self.full_score.map_or(reduced_effect, |score| {
            CriticalityEffect::from_score(score, self.alpha, self.beta)
        });
        let bound_changed = self.full_score.map_or("", |_| {
            if reduced_effect != full_effect {
                "1"
            } else {
                "0"
            }
        });
        let piece_name = if self.piece == PIECE_NONE {
            ".".to_string()
        } else {
            piece_type(self.piece).char_lower().to_string()
        };

        [
            CRITICALITY_SCHEMA_VERSION.to_string(),
            self.decision_kind.as_str().to_string(),
            self.pid.to_string(),
            self.game_id.to_string(),
            self.search_id.to_string(),
            self.root_depth.to_string(),
            self.ply.to_string(),
            self.node_hash.to_string(),
            color_name(self.side_to_move).to_string(),
            move_name(self.m),
            self.from.to_string(),
            self.to.to_string(),
            piece_name,
            self.depth.to_string(),
            self.move_index.to_string(),
            self.base_reduction.to_string(),
            self.final_reduction.to_string(),
            self.new_depth.to_string(),
            self.history_score.to_string(),
            self.static_eval.to_string(),
            has_prev,
            prev,
            delta,
            self.alpha.to_string(),
            self.beta.to_string(),
            bool_int(self.is_pv),
            bool_int(self.is_cut_node),
            bool_int(self.improving),
            bool_int(self.is_killer),
            bool_int(self.is_counter),
            bool_int(self.tt_move_agreement),
            self.label_source.as_str().to_string(),
            reduced,
            full,
            score_delta,
            reduced_effect.as_str().to_string(),
            full_effect.as_str().to_string(),
            bound_changed.to_string(),
        ]
        .join(",")
    }
}

pub struct CriticalityLogger {
    writer: BufWriter<File>,
}

impl CriticalityLogger {
    pub fn open(log_dir: &str) -> io::Result<Option<Self>> {
        if log_dir.trim().is_empty() {
            return Ok(None);
        }

        create_dir_all(log_dir)?;
        let path = Path::new(log_dir).join(format!("criticality-{}.csv", std::process::id()));
        let needs_header = metadata(&path).map_or(true, |meta| meta.len() == 0);
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let mut writer = BufWriter::new(file);
        if needs_header {
            writeln!(writer, "{}", CriticalityRecord::header())?;
        }
        Ok(Some(CriticalityLogger { writer }))
    }

    pub fn write(&mut self, record: &CriticalityRecord) -> io::Result<()> {
        writeln!(self.writer, "{}", record.to_csv_row())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

pub fn should_probe(
    hash: u64,
    m: Move,
    depth: i32,
    ply: usize,
    search_id: u64,
    permille: u32,
) -> bool {
    if permille == 0 {
        return false;
    }
    if permille >= 1000 {
        return true;
    }
    criticality_sample_bucket(hash, m, depth, ply, search_id) < permille
}

pub fn criticality_sample_bucket(
    hash: u64,
    m: Move,
    depth: i32,
    ply: usize,
    search_id: u64,
) -> u32 {
    let mut x = hash
        ^ ((m as u64) << 17)
        ^ ((depth as u64) << 41)
        ^ ((ply as u64) << 53)
        ^ search_id.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x % 1000) as u32
}

fn bool_int(value: bool) -> String {
    if value {
        "1".to_string()
    } else {
        "0".to_string()
    }
}

fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "white",
        Color::Black => "black",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{make_move, Color};

    #[test]
    fn sampler_respects_extreme_permille_values() {
        let m = make_move(12, 28);
        assert!(!should_probe(1, m, 8, 3, 4, 0));
        assert!(should_probe(1, m, 8, 3, 4, 1000));
    }

    #[test]
    fn sampler_is_deterministic() {
        let m = make_move(12, 28);
        assert_eq!(
            criticality_sample_bucket(123, m, 9, 4, 55),
            criticality_sample_bucket(123, m, 9, 4, 55)
        );
    }

    #[test]
    fn record_row_matches_header_width() {
        let m = make_move(12, 28);
        let record = CriticalityRecord {
            decision_kind: CriticalityDecisionKind::Lmr,
            pid: 1,
            game_id: 2,
            search_id: 3,
            root_depth: 6,
            ply: 2,
            node_hash: 123,
            side_to_move: Color::White,
            m,
            from: 12,
            to: 28,
            piece: crate::types::make_piece(Color::White, crate::types::PieceType::Pawn),
            depth: 5,
            move_index: 7,
            base_reduction: 2,
            final_reduction: 1,
            new_depth: 3,
            history_score: 42,
            static_eval: 10,
            prev_static_eval: Some(-5),
            alpha: -20,
            beta: 30,
            is_pv: false,
            is_cut_node: true,
            improving: true,
            is_killer: false,
            is_counter: false,
            tt_move_agreement: false,
            label_source: CriticalityLabelSource::CounterfactualProbe,
            reduced_score: Some(-10),
            full_score: Some(35),
        };
        assert_eq!(
            CriticalityRecord::header().split(',').count(),
            record.to_csv_row().split(',').count()
        );
    }
}
