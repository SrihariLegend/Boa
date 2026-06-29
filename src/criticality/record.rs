use super::*;
pub const CRITICALITY_SCHEMA_VERSION: u32 = 2;

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
    Futility,
}

impl CriticalityDecisionKind {
    fn as_str(self) -> &'static str {
        match self {
            CriticalityDecisionKind::Lmr => "lmr",
            CriticalityDecisionKind::Futility => "futility",
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
    pub futility_margin: Option<i32>,
    pub static_alpha_margin: Option<i32>,
    pub is_pv: bool,
    pub is_cut_node: bool,
    pub improving: bool,
    pub is_killer: bool,
    pub is_counter: bool,
    pub tt_move_agreement: bool,
    pub label_source: CriticalityLabelSource,
    pub reduced_score: Option<i32>,
    pub full_score: Option<i32>,
    /// Position variance σ(pos) at the pruning decision point (centipawns).
    /// None for records where σ was not available (legacy or non-futility decisions).
    pub sigma: Option<i32>,
}

impl CriticalityRecord {
    pub fn header() -> &'static str {
        concat!(
            "schema_version,decision_kind,pid,game_id,search_id,root_depth,ply,node_hash,side_to_move,move_uci,",
            "from,to,piece_type,depth,move_index,base_reduction,final_reduction,new_depth,",
            "history_score,static_eval,has_prev_static_eval,prev_static_eval,static_eval_delta,",
            "alpha,beta,futility_margin,static_alpha_margin,is_pv,is_cut_node,improving,is_killer,is_counter,tt_move_agreement,",
            "label_source,reduced_score,full_score,score_delta_cp,reduced_effect,full_effect,bound_changed,",
            "sigma",
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
            self.futility_margin
                .map_or_else(String::new, |margin| margin.to_string()),
            self.static_alpha_margin
                .map_or_else(String::new, |margin| margin.to_string()),
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
            self.sigma
                .map_or_else(String::new, |s| s.to_string()),
        ]
        .join(",")
    }
}
