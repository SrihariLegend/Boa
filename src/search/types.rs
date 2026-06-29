use super::*;
#[derive(Clone, Copy)]
pub struct Limits {
    pub max_depth: u32,
    pub nodes: u64,     // 0 = unlimited
    pub move_time: u64, // milliseconds, 0 = unlimited
    pub wtime: i64,
    pub btime: i64,
    pub winc: i64,
    pub binc: i64,
    pub moves_to_go: i32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_depth: 64,
            nodes: 0,
            move_time: 0,
            wtime: 0,
            btime: 0,
            winc: 0,
            binc: 0,
            moves_to_go: 0,
        }
    }
}

// ---- Search result ----

pub struct SearchResult {
    pub best_move: Move,
    pub score: Score,
    #[allow(dead_code)]
    pub depth: u32,
    pub nodes: u64,
    #[allow(dead_code)]
    pub pv: Vec<Move>,
}

#[derive(Clone, Copy)]
pub(in crate::search) struct SearchNode {
    pub(in crate::search) alpha: Score,
    pub(in crate::search) beta: Score,
    pub(in crate::search) depth: i32,
    pub(in crate::search) ply: usize,
    pub(in crate::search) is_pv: bool,
}

#[derive(Clone, Copy)]
pub(in crate::search) struct LmrInput {
    pub(in crate::search) moves_searched: usize,
    pub(in crate::search) move_index: usize,
    pub(in crate::search) ply: usize,
    pub(in crate::search) depth: i32,
    pub(in crate::search) history_score: i32,
    pub(in crate::search) static_eval: Score,
    pub(in crate::search) prev_static_eval: Option<Score>,
    pub(in crate::search) alpha: Score,
    pub(in crate::search) beta: Score,
    pub(in crate::search) root_depth: i32,
    pub(in crate::search) side_to_move: Color,
    pub(in crate::search) moving_piece: Piece,
    pub(in crate::search) is_pv: bool,
    pub(in crate::search) is_cut_node: bool,
    pub(in crate::search) improving: bool,
    pub(in crate::search) is_killer: bool,
    pub(in crate::search) is_counter: bool,
    pub(in crate::search) tt_move_agreement: bool,
    pub(in crate::search) is_capture: bool,
    pub(in crate::search) is_promo: bool,
    pub(in crate::search) gives_check: bool,
    pub(in crate::search) in_check: bool,
}

#[derive(Clone, Copy)]
pub(in crate::search) struct LmrReduction {
    pub(in crate::search) base_reduction: i32,
    pub(in crate::search) final_reduction: i32,
}

#[derive(Clone, Copy)]
pub struct FfpInput {
    pub depth: i32,
    pub static_eval: Score,
    pub alpha: Score,
    pub move_index: usize,
    pub is_cut_node: bool,
    pub history_score: i32,
    pub sigma: i32,
}

pub(in crate::search) struct CriticalityRecordInput {
    pub(in crate::search) enabled: bool,
    pub(in crate::search) node_hash: u64,
    pub(in crate::search) side_to_move: Color,
    pub(in crate::search) m: Move,
    pub(in crate::search) ply: usize,
    pub(in crate::search) from: Square,
    pub(in crate::search) to: Square,
    pub(in crate::search) moving_piece: Piece,
    pub(in crate::search) depth: i32,
    pub(in crate::search) move_index: usize,
    pub(in crate::search) base_reduction: i32,
    pub(in crate::search) final_reduction: i32,
    pub(in crate::search) new_depth: i32,
    pub(in crate::search) history_score: i32,
    pub(in crate::search) static_eval: Score,
    pub(in crate::search) prev_static_eval: Option<Score>,
    pub(in crate::search) alpha: Score,
    pub(in crate::search) beta: Score,
    pub(in crate::search) is_pv: bool,
    pub(in crate::search) is_cut_node: bool,
    pub(in crate::search) improving: bool,
    pub(in crate::search) is_killer: bool,
    pub(in crate::search) is_counter: bool,
    pub(in crate::search) tt_move_agreement: bool,
    pub(in crate::search) sigma: Option<i32>,
}

// ---- Search context ----
