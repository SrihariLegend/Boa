// Probe event types — all diagnostic events across every engine module.
//
// Each struct maps to one `typ` code in the JSONL output.  Field names use
// short codes (serde rename) for AI token efficiency.  The meta header in
// each output file provides the field legend.
//
// Adding a new field: add Option<T> with skip_serializing_if.
// Adding a new module: add struct, add variant to ProbeEvent, add probe!() call.

#[cfg(feature = "probes")]
use serde::Serialize;

// ============================================================
// 1. Config — typ:"cf" — fires once at search start
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct ConfigEvent {
    #[cfg_attr(feature = "probes", serde(rename = "ms"))]
    pub tt_size_mb: u32,
    #[cfg_attr(feature = "probes", serde(rename = "ma"))]
    pub material_scale: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ps"))]
    pub pst_scale: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mo"))]
    pub mobility_scale: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ks"))]
    pub king_safety_scale: i32,
    #[cfg_attr(feature = "probes", serde(rename = "pa"))]
    pub pawn_structure_scale: i32,
    #[cfg_attr(feature = "probes", serde(rename = "co"))]
    pub contempt: i32,
    #[cfg_attr(feature = "probes", serde(rename = "sy"))]
    pub syzygy_enabled: bool,
    #[cfg_attr(feature = "probes", serde(rename = "md"))]
    pub max_depth: u32,
    #[cfg_attr(feature = "probes", serde(rename = "mt"))]
    pub move_time: u64,
    #[cfg_attr(feature = "probes", serde(rename = "wt"))]
    pub wtime: i64,
    #[cfg_attr(feature = "probes", serde(rename = "bt"))]
    pub btime: i64,
    #[cfg_attr(feature = "probes", serde(rename = "wi"))]
    pub winc: i64,
    #[cfg_attr(feature = "probes", serde(rename = "bi"))]
    pub binc: i64,
    #[cfg_attr(feature = "probes", serde(rename = "mg"))]
    pub moves_to_go: i32,
}

// ============================================================
// 2. Board — typ:"b" — fires on position set
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct BoardEvent {
    /// FEN truncated to 64 chars
    #[cfg_attr(feature = "probes", serde(rename = "f"))]
    pub fen: String,
    #[cfg_attr(feature = "probes", serde(rename = "p"))]
    pub phase: i32,
    #[cfg_attr(feature = "probes", serde(rename = "nm"))]
    pub non_pawn_material: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mo"))]
    pub mobile_pieces: i32,
    #[cfg_attr(feature = "probes", serde(rename = "of"))]
    pub open_files: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ck"))]
    pub in_check: bool,
    #[cfg_attr(feature = "probes", serde(rename = "mr"))]
    pub material_rule_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "hm"))]
    pub halfmove_clock: i32,
    #[cfg_attr(feature = "probes", serde(rename = "fl"))]
    pub fullmove_number: i32,
}

// ============================================================
// 3. Movegen — typ:"mg" — fires per position
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct MovegenEvent {
    #[cfg_attr(feature = "probes", serde(rename = "nc"))]
    pub total_count: u32,
    #[cfg_attr(feature = "probes", serde(rename = "qc"))]
    pub quiet_count: u32,
    #[cfg_attr(feature = "probes", serde(rename = "cc"))]
    pub capture_count: u32,
    #[cfg_attr(feature = "probes", serde(rename = "ec"))]
    pub evasion_count: u32,
    #[cfg_attr(feature = "probes", serde(rename = "pc"))]
    pub promotion_count: u32,
    #[cfg_attr(feature = "probes", serde(rename = "ck"))]
    pub in_check: bool,
}

// ============================================================
// 4. Eval — typ:"ev" — full EvalBreakdown
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct EvalEvent {
    #[cfg_attr(feature = "probes", serde(rename = "ph"))]
    pub phase: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ma_mg"))]
    pub material_mg: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ma_eg"))]
    pub material_eg: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ma_cp"))]
    pub material_cp: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ps_mg"))]
    pub pst_mg: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ps_eg"))]
    pub pst_eg: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ps_cp"))]
    pub pst_cp: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mo_mg"))]
    pub mobility_mg: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mo_eg"))]
    pub mobility_eg: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mo_cp"))]
    pub mobility_cp: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mw"))]
    pub mobility_white: u32,
    #[cfg_attr(feature = "probes", serde(rename = "mb"))]
    pub mobility_black: u32,
    #[cfg_attr(feature = "probes", serde(rename = "pa_mg"))]
    pub pawn_structure_mg: i32,
    #[cfg_attr(feature = "probes", serde(rename = "pa_eg"))]
    pub pawn_structure_eg: i32,
    #[cfg_attr(feature = "probes", serde(rename = "pa_cp"))]
    pub pawn_structure_cp: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ks_mg"))]
    pub king_safety_mg: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ks_eg"))]
    pub king_safety_eg: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ks_cp"))]
    pub king_safety_cp: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ws"))]
    pub white_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ss"))]
    pub side_to_move_score: i32,
}

// ============================================================
// 5. Search Node — typ:"sn" — per-node state
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct SearchNodeEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "p"))]
    pub ply: u32,
    #[cfg_attr(feature = "probes", serde(rename = "se"))]
    pub static_eval: i32,
    #[cfg_attr(feature = "probes", serde(rename = "a"))]
    pub alpha: i32,
    #[cfg_attr(feature = "probes", serde(rename = "b"))]
    pub beta: i32,
    #[cfg_attr(feature = "probes", serde(rename = "pv"))]
    pub is_pv: bool,
    #[cfg_attr(feature = "probes", serde(rename = "cu"))]
    pub is_cut_node: bool,
    #[cfg_attr(feature = "probes", serde(rename = "ck"))]
    pub in_check: bool,
    #[cfg_attr(feature = "probes", serde(rename = "im"))]
    pub improving: bool,
    #[cfg_attr(feature = "probes", serde(rename = "ps"))]
    pub prev_static_eval: Option<i32>,
    #[cfg_attr(feature = "probes", serde(rename = "sc"))]
    pub score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "nm"))]
    pub moves_searched: u32,
    #[cfg_attr(feature = "probes", serde(rename = "bf"))]
    pub beta_cutoffs_this_node: u32,
    #[cfg_attr(feature = "probes", serde(rename = "fc"))]
    pub first_move_cutoff: bool,
    #[cfg_attr(feature = "probes", serde(rename = "tm"))]
    pub node_time_us: u64,
    #[cfg_attr(feature = "probes", serde(rename = "tb"))]
    pub tb_hit: bool,
    #[cfg_attr(feature = "probes", serde(rename = "tt"))]
    pub tt_hit: bool,
}

// ============================================================
// 6. Search Summary — typ:"ss" — fires once at search end
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct SearchSummaryEvent {
    #[cfg_attr(feature = "probes", serde(rename = "td"))]
    pub depth_completed: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ns"))]
    pub total_nodes: u64,
    #[cfg_attr(feature = "probes", serde(rename = "qs"))]
    pub qsearch_nodes: u64,
    #[cfg_attr(feature = "probes", serde(rename = "tm"))]
    pub time_ms: u64,
    #[cfg_attr(feature = "probes", serde(rename = "np"))]
    pub nodes_per_sec: u64,
    #[cfg_attr(feature = "probes", serde(rename = "bm"))]
    pub best_move: String,
    #[cfg_attr(feature = "probes", serde(rename = "bs"))]
    pub best_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "sd"))]
    pub sel_depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "tt_p"))]
    pub tt_probes: u64,
    #[cfg_attr(feature = "probes", serde(rename = "tt_h"))]
    pub tt_hits: u64,
    #[cfg_attr(feature = "probes", serde(rename = "tt_c"))]
    pub tt_cutoffs: u64,
    #[cfg_attr(feature = "probes", serde(rename = "bc"))]
    pub beta_cutoffs: u64,
    #[cfg_attr(feature = "probes", serde(rename = "fc"))]
    pub first_move_cutoffs: u64,
    #[cfg_attr(feature = "probes", serde(rename = "nm_t"))]
    pub null_move_tries: u64,
    #[cfg_attr(feature = "probes", serde(rename = "nm_c"))]
    pub null_move_cutoffs: u64,
    #[cfg_attr(feature = "probes", serde(rename = "rp"))]
    pub rfp_cutoffs: u64,
    #[cfg_attr(feature = "probes", serde(rename = "fp_a"))]
    pub ffp_attempts: u64,
    #[cfg_attr(feature = "probes", serde(rename = "fp_p"))]
    pub ffp_prunes: u64,
    #[cfg_attr(feature = "probes", serde(rename = "lm_a"))]
    pub lmr_attempts: u64,
    #[cfg_attr(feature = "probes", serde(rename = "lm_r"))]
    pub lmr_actual_reductions: u64,
    #[cfg_attr(feature = "probes", serde(rename = "lm_rs"))]
    pub lmr_re_searches: u64,
    #[cfg_attr(feature = "probes", serde(rename = "se_w"))]
    pub see_win_caps: u64,
    #[cfg_attr(feature = "probes", serde(rename = "se_e"))]
    pub see_equal_caps: u64,
    #[cfg_attr(feature = "probes", serde(rename = "se_l"))]
    pub see_loss_caps: u64,
    #[cfg_attr(feature = "probes", serde(rename = "se_s"))]
    pub see_loss_searched: u64,
    #[cfg_attr(feature = "probes", serde(rename = "ii_t"))]
    pub iid_triggers: u64,
    #[cfg_attr(feature = "probes", serde(rename = "ii_s"))]
    pub iid_successes: u64,
    #[cfg_attr(feature = "probes", serde(rename = "tb_h"))]
    pub tb_hits: u64,
    #[cfg_attr(feature = "probes", serde(rename = "dr"))]
    pub dropped_probe_events: u64,
}

// ============================================================
// 7. TT Probe — typ:"tt"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct TtProbeEvent {
    #[cfg_attr(feature = "probes", serde(rename = "op"))]
    pub operation: &'static str, // "probe" or "store"
    #[cfg_attr(feature = "probes", serde(rename = "h"))]
    pub hit: bool,
    #[cfg_attr(feature = "probes", serde(rename = "et"))]
    pub entry_type: &'static str, // "exact", "alpha", "beta", "empty"
    #[cfg_attr(feature = "probes", serde(rename = "ed"))]
    pub entry_depth: i8,
    #[cfg_attr(feature = "probes", serde(rename = "es"))]
    pub entry_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ag"))]
    pub entry_age: u8,
    #[cfg_attr(feature = "probes", serde(rename = "si"))]
    pub slot_index: u8,
    #[cfg_attr(feature = "probes", serde(rename = "re"))]
    pub replaced: bool,
    #[cfg_attr(feature = "probes", serde(rename = "rd"))]
    pub replaced_depth: i8,
}

// ============================================================
// 8. TT Cutoff — typ:"tc"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct TtCutoffEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "et"))]
    pub entry_type: &'static str,
    #[cfg_attr(feature = "probes", serde(rename = "ed"))]
    pub entry_depth: i8,
    #[cfg_attr(feature = "probes", serde(rename = "df"))]
    pub depth_sufficient: bool,
    #[cfg_attr(feature = "probes", serde(rename = "sc"))]
    pub cutoff_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "a"))]
    pub alpha: i32,
    #[cfg_attr(feature = "probes", serde(rename = "b"))]
    pub beta: i32,
}

// ============================================================
// 9. FFP — typ:"fp"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct FfpEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mi"))]
    pub move_index: u32,
    #[cfg_attr(feature = "probes", serde(rename = "hs"))]
    pub history_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mg"))]
    pub computed_margin: i32,
    #[cfg_attr(feature = "probes", serde(rename = "rg"))]
    pub required_gain: i32,
    #[cfg_attr(feature = "probes", serde(rename = "pr"))]
    pub pruned: bool,
    #[cfg_attr(feature = "probes", serde(rename = "cu"))]
    pub is_cut_node: bool,
}

// ============================================================
// 10. RFP — typ:"rp"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct RfpEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "se"))]
    pub static_eval: i32,
    #[cfg_attr(feature = "probes", serde(rename = "b"))]
    pub beta: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mg"))]
    pub computed_margin: i32,
    #[cfg_attr(feature = "probes", serde(rename = "pr"))]
    pub pruned: bool,
}

// ============================================================
// 11. LMR — typ:"lm"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct LmrEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "p"))]
    pub ply: u32,
    #[cfg_attr(feature = "probes", serde(rename = "mi"))]
    pub move_index: u32,
    #[cfg_attr(feature = "probes", serde(rename = "ms"))]
    pub moves_searched: u32,
    #[cfg_attr(feature = "probes", serde(rename = "hs"))]
    pub history_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "br"))]
    pub base_reduction: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ar"))]
    pub actual_reduction: i32,
    #[cfg_attr(feature = "probes", serde(rename = "nd"))]
    pub new_depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ip"))]
    pub improving: bool,
    #[cfg_attr(feature = "probes", serde(rename = "ki"))]
    pub is_killer: bool,
    #[cfg_attr(feature = "probes", serde(rename = "co"))]
    pub is_counter: bool,
    #[cfg_attr(feature = "probes", serde(rename = "tm"))]
    pub tt_move_agreement: bool,
    #[cfg_attr(feature = "probes", serde(rename = "gc"))]
    pub gives_check: bool,
    #[cfg_attr(feature = "probes", serde(rename = "pi"))]
    pub moving_piece: u8,
    #[cfg_attr(feature = "probes", serde(rename = "cu"))]
    pub is_cut_node: bool,
}

// ============================================================
// 12. Null Move — typ:"nm"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct NullMoveEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "se"))]
    pub static_eval: i32,
    #[cfg_attr(feature = "probes", serde(rename = "b"))]
    pub beta: i32,
    #[cfg_attr(feature = "probes", serde(rename = "r"))]
    pub reduction: i32,
    #[cfg_attr(feature = "probes", serde(rename = "sc"))]
    pub null_move_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "pr"))]
    pub pruned: bool,
}

// ============================================================
// 13. SEE — typ:"se"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct SeeEvent {
    #[cfg_attr(feature = "probes", serde(rename = "vl"))]
    pub see_value: i32,
    #[cfg_attr(feature = "probes", serde(rename = "cv"))]
    pub captured_value: i32,
    #[cfg_attr(feature = "probes", serde(rename = "th"))]
    pub threshold: i32,
    #[cfg_attr(feature = "probes", serde(rename = "pr"))]
    pub pruned_by_see: bool,
    #[cfg_attr(feature = "probes", serde(rename = "sr"))]
    pub searched_despite_bad_see: bool,
    #[cfg_attr(feature = "probes", serde(rename = "px"))]
    pub pin_excluded: bool,
}

// ============================================================
// 15. Quiescence — typ:"qs"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct QuiescenceEvent {
    #[cfg_attr(feature = "probes", serde(rename = "p"))]
    pub ply: u32,
    #[cfg_attr(feature = "probes", serde(rename = "sp"))]
    pub stand_pat_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "a"))]
    pub alpha: i32,
    #[cfg_attr(feature = "probes", serde(rename = "b"))]
    pub beta: i32,
    #[cfg_attr(feature = "probes", serde(rename = "sc"))]
    pub final_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "nc"))]
    pub captures_searched: u32,
    #[cfg_attr(feature = "probes", serde(rename = "dp"))]
    pub delta_pruned_count: u32,
    #[cfg_attr(feature = "probes", serde(rename = "se"))]
    pub see_pruned_count: u32,
    #[cfg_attr(feature = "probes", serde(rename = "ck"))]
    pub in_check: bool,
    #[cfg_attr(feature = "probes", serde(rename = "fc"))]
    pub futility_cutoff: bool,
}

// ============================================================
// 16. Aspiration — typ:"aw"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct AspirationEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "dl"))]
    pub initial_delta: i32,
    #[cfg_attr(feature = "probes", serde(rename = "lo"))]
    pub window_low: i32,
    #[cfg_attr(feature = "probes", serde(rename = "hi"))]
    pub window_high: i32,
    #[cfg_attr(feature = "probes", serde(rename = "fh"))]
    pub fail_high: bool,
    #[cfg_attr(feature = "probes", serde(rename = "fl"))]
    pub fail_low: bool,
    #[cfg_attr(feature = "probes", serde(rename = "ex"))]
    pub expansion_count: u32,
    #[cfg_attr(feature = "probes", serde(rename = "rs"))]
    pub research_score: i32,
}

// ============================================================
// 17. IID — typ:"ii"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct IidEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "rd"))]
    pub reduced_depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "tf"))]
    pub tt_move_found_after_iid: bool,
    #[cfg_attr(feature = "probes", serde(rename = "sc"))]
    pub iid_search_score: i32,
}

// ============================================================
// 18. Move Ordering — typ:"mo"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct MoveOrderingEvent {
    #[cfg_attr(feature = "probes", serde(rename = "p"))]
    pub ply: u32,
    #[cfg_attr(feature = "probes", serde(rename = "mi"))]
    pub move_index: u32,
    #[cfg_attr(feature = "probes", serde(rename = "ph"))]
    pub phase_picked: &'static str, // "tt","hash","good_cap","killer","counter","quiet","bad_cap"
    #[cfg_attr(feature = "probes", serde(rename = "bf"))]
    pub butterfly_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "kh"))]
    pub killer_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ch"))]
    pub counter_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ca"))]
    pub capture_history_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mv"))]
    pub mvv_lva_base: i32,
    #[cfg_attr(feature = "probes", serde(rename = "tt"))]
    pub tt_move_bonus: bool,
    #[cfg_attr(feature = "probes", serde(rename = "pr"))]
    pub promotion_bonus: bool,
}

// ============================================================
// 19. History Table — typ:"ht"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct HistoryEvent {
    #[cfg_attr(feature = "probes", serde(rename = "ev"))]
    pub event_type: &'static str, // "overflow", "scale_down", "cap_scale_down"
    #[cfg_attr(feature = "probes", serde(rename = "ci"))]
    pub color_index: u8,
    #[cfg_attr(feature = "probes", serde(rename = "pi"))]
    pub piece_index: u8,
    #[cfg_attr(feature = "probes", serde(rename = "mx"))]
    pub max_value_before: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mn"))]
    pub min_value_before: i32,
    #[cfg_attr(feature = "probes", serde(rename = "th"))]
    pub threshold: i32,
}

// ============================================================
// 20. Root — typ:"rt"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct RootEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "bm"))]
    pub best_move: String,
    #[cfg_attr(feature = "probes", serde(rename = "bs"))]
    pub best_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "pv"))]
    pub pv_line: String,
    #[cfg_attr(feature = "probes", serde(rename = "bc"))]
    pub best_move_changed: bool,
    #[cfg_attr(feature = "probes", serde(rename = "pc"))]
    pub previous_best_move: String,
    #[cfg_attr(feature = "probes", serde(rename = "it"))]
    pub iteration_time_ms: u64,
    #[cfg_attr(feature = "probes", serde(rename = "ns"))]
    pub nodes_this_iteration: u64,
    #[cfg_attr(feature = "probes", serde(rename = "af"))]
    pub aspiration_fails: u32,
}

// ============================================================
// 21. Time Management — typ:"tm"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct TimeIterationEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "st"))]
    pub stability: u32,
    #[cfg_attr(feature = "probes", serde(rename = "sf"))]
    pub stability_factor: f64,
    #[cfg_attr(feature = "probes", serde(rename = "sd"))]
    pub score_delta: i32,
    #[cfg_attr(feature = "probes", serde(rename = "sc"))]
    pub score_factor: f64,
    #[cfg_attr(feature = "probes", serde(rename = "nb"))]
    pub not_best_pct: f64,
    #[cfg_attr(feature = "probes", serde(rename = "nf"))]
    pub node_factor: f64,
    #[cfg_attr(feature = "probes", serde(rename = "cf"))]
    pub combined_factor: f64,
    #[cfg_attr(feature = "probes", serde(rename = "at"))]
    pub adjusted_time: u64,
    #[cfg_attr(feature = "probes", serde(rename = "dc"))]
    pub decision: String,
}

pub struct TimeManagementEvent {
    #[cfg_attr(feature = "probes", serde(rename = "al"))]
    pub allocated: u64,
    #[cfg_attr(feature = "probes", serde(rename = "ha"))]
    pub hard_limit: u64,
    #[cfg_attr(feature = "probes", serde(rename = "op"))]
    pub optimum_time: u64,
    #[cfg_attr(feature = "probes", serde(rename = "el"))]
    pub elapsed: u64,
    #[cfg_attr(feature = "probes", serde(rename = "mt"))]
    pub moves_to_go: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mp"))]
    pub move_overhead: i64,
    #[cfg_attr(feature = "probes", serde(rename = "rm"))]
    pub remaining_clock: i64,
    #[cfg_attr(feature = "probes", serde(rename = "ic"))]
    pub increment: i64,
}

// ============================================================
// 22. Syzygy — typ:"tz"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct SyzygyEvent {
    #[cfg_attr(feature = "probes", serde(rename = "rs"))]
    pub result: &'static str, // "win","draw","loss","cursed","blessed","not_found"
    #[cfg_attr(feature = "probes", serde(rename = "dm"))]
    pub distance_to_mate: i32,
    #[cfg_attr(feature = "probes", serde(rename = "pc"))]
    pub piece_count: u8,
    #[cfg_attr(feature = "probes", serde(rename = "dz"))]
    pub dtz_value: i32,
    #[cfg_attr(feature = "probes", serde(rename = "wp"))]
    pub wdl_probe_success: bool,
}

// ============================================================
// 23. Draw Detection — typ:"dd"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct DrawEvent {
    #[cfg_attr(feature = "probes", serde(rename = "ty"))]
    pub draw_type: &'static str, // "repetition","fifty_move","insufficient_material"
    #[cfg_attr(feature = "probes", serde(rename = "p"))]
    pub ply: u32,
    #[cfg_attr(feature = "probes", serde(rename = "co"))]
    pub contempt_applied: i32,
    #[cfg_attr(feature = "probes", serde(rename = "sc"))]
    pub score_returned: i32,
}

// ============================================================
// 24. Mate Distance — typ:"md"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct MateDistanceEvent {
    #[cfg_attr(feature = "probes", serde(rename = "p"))]
    pub ply: u32,
    #[cfg_attr(feature = "probes", serde(rename = "oa"))]
    pub original_alpha: i32,
    #[cfg_attr(feature = "probes", serde(rename = "na"))]
    pub clamped_alpha: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ob"))]
    pub original_beta: i32,
    #[cfg_attr(feature = "probes", serde(rename = "nb"))]
    pub clamped_beta: i32,
    #[cfg_attr(feature = "probes", serde(rename = "pr"))]
    pub pruned: bool,
}

// ============================================================
// 25. Continuation History — typ:"ch"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct ContHistoryEvent {
    #[cfg_attr(feature = "probes", serde(rename = "tb"))]
    pub table: &'static str, // "cont1", "cont2", "cont4", "cont6"
    #[cfg_attr(feature = "probes", serde(rename = "hr"))]
    pub hit_rate: f64, // fraction of quiet moves with non-zero score
    #[cfg_attr(feature = "probes", serde(rename = "as"))]
    pub avg_score: f64, // average contribution to move score (abs)
    #[cfg_attr(feature = "probes", serde(rename = "mx"))]
    pub max_abs: i32, // max absolute value in table
    #[cfg_attr(feature = "probes", serde(rename = "uf"))]
    pub update_freq: u64, // bonus+malus updates this search
}

// ============================================================
// 26. Correction History — typ:"cr"
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct CorrectionHistoryEvent {
    #[cfg_attr(feature = "probes", serde(rename = "cv"))]
    pub correction_value: i32, // total correction (in corr units; /512 → cp)
    #[cfg_attr(feature = "probes", serde(rename = "re"))]
    pub raw_eval: i32, // raw static eval before correction
    #[cfg_attr(feature = "probes", serde(rename = "ce"))]
    pub corrected_eval: i32, // eval after correction
    #[cfg_attr(feature = "probes", serde(rename = "df"))]
    pub diff: i32, // best_score - raw_eval (the correction update)
    #[cfg_attr(feature = "probes", serde(rename = "pc"))]
    pub pawn_corr: i32, // pawn correction component
    #[cfg_attr(feature = "probes", serde(rename = "np"))]
    pub nonpawn_corr: i32, // non-pawn correction component
    #[cfg_attr(feature = "probes", serde(rename = "cc"))]
    pub cont_corr: i32, // continuation correction component
    #[cfg_attr(feature = "probes", serde(rename = "pl"))]
    pub ply: u32,
}

// ============================================================
// Master enum — serde(tag = "typ") produces {"typ":"fp",...fields}
// ============================================================
// ============================================================
// 21. ProbCut — typ:"pc" — fires when ProbCut is evaluated
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct ProbCutEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "b"))]
    pub beta: i32,
    #[cfg_attr(feature = "probes", serde(rename = "pb"))]
    pub prob_beta: i32,
    #[cfg_attr(feature = "probes", serde(rename = "se"))]
    pub static_eval: i32,
    #[cfg_attr(feature = "probes", serde(rename = "at"))]
    pub attempts: u32,
    #[cfg_attr(feature = "probes", serde(rename = "ac"))]
    pub accepted: bool,
    #[cfg_attr(feature = "probes", serde(rename = "ps"))]
    pub prob_score: Option<i32>,
    #[cfg_attr(feature = "probes", serde(rename = "ns"))]
    pub nodes_saved: Option<u64>,
}

#[cfg_attr(feature = "probes", derive(Serialize))]
#[cfg_attr(feature = "probes", serde(tag = "typ"))]
pub enum ProbeEvent {
    #[cfg_attr(feature = "probes", serde(rename = "cf"))]
    Config(ConfigEvent),
    #[cfg_attr(feature = "probes", serde(rename = "b"))]
    Board(BoardEvent),
    #[cfg_attr(feature = "probes", serde(rename = "mg"))]
    Movegen(MovegenEvent),
    #[cfg_attr(feature = "probes", serde(rename = "ev"))]
    Eval(EvalEvent),
    #[cfg_attr(feature = "probes", serde(rename = "sn"))]
    SearchNode(SearchNodeEvent),
    #[cfg_attr(feature = "probes", serde(rename = "ss"))]
    SearchSummary(SearchSummaryEvent),
    #[cfg_attr(feature = "probes", serde(rename = "tt"))]
    TtProbe(TtProbeEvent),
    #[cfg_attr(feature = "probes", serde(rename = "tc"))]
    TtCutoff(TtCutoffEvent),
    #[cfg_attr(feature = "probes", serde(rename = "fp"))]
    Ffp(FfpEvent),
    #[cfg_attr(feature = "probes", serde(rename = "rp"))]
    Rfp(RfpEvent),
    #[cfg_attr(feature = "probes", serde(rename = "lm"))]
    Lmr(LmrEvent),
    #[cfg_attr(feature = "probes", serde(rename = "nm"))]
    NullMove(NullMoveEvent),
    #[cfg_attr(feature = "probes", serde(rename = "se"))]
    See(SeeEvent),
    #[cfg_attr(feature = "probes", serde(rename = "qs"))]
    Quiescence(QuiescenceEvent),
    #[cfg_attr(feature = "probes", serde(rename = "aw"))]
    Aspiration(AspirationEvent),
    #[cfg_attr(feature = "probes", serde(rename = "ii"))]
    Iid(IidEvent),
    #[cfg_attr(feature = "probes", serde(rename = "mo"))]
    MoveOrdering(MoveOrderingEvent),
    #[cfg_attr(feature = "probes", serde(rename = "ht"))]
    History(HistoryEvent),
    #[cfg_attr(feature = "probes", serde(rename = "rt"))]
    Root(RootEvent),
    #[cfg_attr(feature = "probes", serde(rename = "ti"))]
    TimeIteration(TimeIterationEvent),
    #[cfg_attr(feature = "probes", serde(rename = "tm"))]
    TimeManagement(TimeManagementEvent),
    #[cfg_attr(feature = "probes", serde(rename = "tz"))]
    Syzygy(SyzygyEvent),
    #[cfg_attr(feature = "probes", serde(rename = "dd"))]
    DrawDetection(DrawEvent),
    #[cfg_attr(feature = "probes", serde(rename = "md"))]
    MateDistance(MateDistanceEvent),
    #[cfg_attr(feature = "probes", serde(rename = "ch"))]
    ContHistory(ContHistoryEvent),
    #[cfg_attr(feature = "probes", serde(rename = "cr"))]
    CorrectionHistory(CorrectionHistoryEvent),
    #[cfg_attr(feature = "probes", serde(rename = "pc"))]
    ProbCut(ProbCutEvent),

    /// Sentinel: tells the writer thread to flush and end the file.

    #[cfg_attr(feature = "probes", serde(rename = "sx"))]
    SingularExtension(SingularExtensionEvent),
    #[cfg_attr(feature = "probes", serde(rename = "te"))]
    ThreatExtension(ThreatExtensionEvent),
    #[cfg_attr(feature = "probes", serde(rename = "re"))]
    RecaptureExtension(RecaptureExtensionEvent),
    #[cfg_attr(feature = "probes", serde(rename = "xx"))]
    Finish,
}

#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct SingularExtensionEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "tt"))]
    pub tt_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "sb"))]
    pub singular_beta: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ss"))]
    pub singular_score: i32,
    #[cfg_attr(feature = "probes", serde(rename = "ext"))]
    pub extension: i32,
    #[cfg_attr(feature = "probes", serde(rename = "mc"))]
    pub multi_cut: bool,
}

#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct ThreatExtensionEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
    #[cfg_attr(feature = "probes", serde(rename = "lr"))]
    pub lmr_reduction: i32,
}

#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct RecaptureExtensionEvent {
    #[cfg_attr(feature = "probes", serde(rename = "d"))]
    pub depth: i32,
}
