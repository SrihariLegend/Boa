// ---- Search tuning constants ----
// Sources: Stockfish (SF), Chess Programming Wiki (CPW), or marked [NEEDS TUNING].

/// Aspiration window: initial half-width in centipawns.
/// SF uses ~10-20 with gradual widening; 25 is a conservative starting point. [NEEDS TUNING]
pub(in crate::search) const ASPIRATION_DELTA: i32 = 25;

/// Aspiration: use full window below this depth (no point aspirating at low depth).
/// Standard practice: SF uses 4-5.
pub(in crate::search) const ASPIRATION_MIN_DEPTH: u32 = 4;

/// Aspiration: maximum window expansions before falling through to a full
/// -INF/+INF re-search. Prevents pathological re-search loops (SF caps at ~4).
pub(in crate::search) const ASPIRATION_MAX_EXPANSIONS: u32 = 4;

// ---- Correction history weights for pruning margins ----
// When correction is large, the eval is unreliable for this position type.
// Widen pruning margins by |corr| * weight / 512 to be safer.
// Override with env vars: BOA_CORR_W_RFP, BOA_CORR_W_NMP, BOA_CORR_W_FFP.
pub(in crate::search) fn corr_w_rfp() -> i32 {
    std::env::var("BOA_CORR_W_RFP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2)
}
pub(in crate::search) fn corr_w_nmp() -> i32 {
    std::env::var("BOA_CORR_W_NMP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}
pub(in crate::search) fn corr_w_ffp() -> i32 {
    std::env::var("BOA_CORR_W_FFP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

// ---- RFP (Reverse Futility Pruning) ----

/// RFP: maximum depth at which to apply. Standard: 5-8 (CPW).
pub(in crate::search) const RFP_MAX_DEPTH: i32 = 5;

/// RFP: margin per depth ply (centipawns). M = RFP_MARGIN_PER_DEPTH * d
///     + CORR_W_RFP * |correction| / 512.
/// Standard range: 50-100 (CPW). [NEEDS SPRT]
pub(in crate::search) const RFP_MARGIN_PER_DEPTH: i32 = 50;

// ---- FFP (Forward Futility Pruning) ----

/// FFP: maximum remaining depth.
pub(in crate::search) const FFP_MAX_DEPTH: i32 = 4;

/// FFP: move-index weight (centipawns). Earlier moves more likely good. [NEEDS SPRT]
pub(in crate::search) const FFP_W_IDX: f64 = 40.0;

/// FFP: history-score coefficient (centipawns). Good-history moves more
/// likely to improve eval. [NEEDS SPRT]
pub(in crate::search) const FFP_W_HIST: f64 = 20.0;

/// FFP: history score normalizer.
pub(in crate::search) const FFP_HISTORY_NORMALIZER: i32 = 32_768;

/// FFP: safety buffer added to the estimated gain (centipawns).
pub(in crate::search) const FFP_BUFFER: i32 = 0;

/// FFP: maximum move rank for log-scaled move-index uncertainty.
pub(in crate::search) const FFP_MAX_RANK: usize = 60;

/// Null-move pruning: minimum depth to attempt.
/// Standard: 3 (CPW, SF).
pub(in crate::search) const NULL_MOVE_MIN_DEPTH: i32 = 3;

/// Late move reductions: minimum moves searched before applying LMR.
/// Classic conservative LMR: reduce only late quiet moves.
pub(in crate::search) const LMR_FULL_DEPTH_MOVES: usize = 4;

/// LMR: minimum depth to start reducing.
/// Standard: 3 (CPW).
pub(in crate::search) const LMR_MIN_DEPTH: i32 = 3;

/// LMR: log-product divisor. Smaller values reduce more aggressively.
pub(in crate::search) const LMR_LOG_DIVISOR: f64 = 2.5;

/// LMR: normalize quiet history into a small reduction adjustment.
/// Clamped at HISTORY_GRAVITY (the natural ceiling under gravity aging).
pub(in crate::search) const LMR_HISTORY_NORMALIZER: i32 = 4_096;

/// LMR: whether to scale reductions by PV/cut-node type.
/// Standard in Stockfish, Ethereal, Berserk: PV gets less reduction.
pub(in crate::search) const LMR_NODE_TYPE_SCALING: bool = true;

/// Quiescence delta pruning margin (centipawns).
/// If stand_pat + capture_value + margin < alpha, skip. ~200 is standard (SF, CPW).
pub(in crate::search) const DELTA_PRUNING_MARGIN: i32 = 200;

/// History gravity constant. History values asymptotically approach ±GRAVITY
/// via the formula: new = old + delta - old * abs(delta) / GRAVITY.
/// 16384 (2¹⁴) is the universal standard across all top engines.
pub(in crate::search) const HISTORY_GRAVITY: i32 = 16_384;

/// Time management: assumed remaining moves when movestogo is not specified.
/// 30 is a common default (CPW). Conservative engines use 25-40.
pub(in crate::search) const DEFAULT_MOVES_TO_GO: i64 = 30;

/// Time management: minimum time allocation in milliseconds.
pub(in crate::search) const MIN_MOVE_TIME_MS: i64 = 10;

/// Time management: hard limit multiplier and additive cap.
/// Prevents flagging by limiting total time to soft_budget * multiplier, capped.
pub(in crate::search) const HARD_TIME_MULTIPLIER: u64 = 5;
pub(in crate::search) const HARD_TIME_ADDITIVE_CAP: u64 = 2000;

/// Time management: reserve for GUI/process latency per move. Without this,
/// the engine budgets 100% of the clock and forfeits on time at fast TCs.
pub(in crate::search) const MOVE_OVERHEAD_MS: i64 = 30;

/// Internal Iterative Deepening: minimum depth to apply IID.
/// When no TT move is available, do a reduced search to find a candidate.
/// Standard: 4-6 (CPW). Applied at PV nodes and high-depth non-PV nodes.
pub(in crate::search) const IID_MIN_DEPTH: i32 = 5;

/// IID: depth reduction for the internal search.
/// Common formula: depth - 2 or depth - depth/4 - 1.
pub(in crate::search) const IID_REDUCTION: i32 = 2;

/// Capture history: scale divisor when adding to MVV-LVA score.
/// Keeps learned capture ordering from overwhelming the static MVV-LVA signal.
pub(in crate::search) const CAP_HISTORY_DIVISOR: i32 = 16;

// Quiescence check evasion: evasions are searched without a ply cap (relying
// on MAX_PLY for termination). In-check stand-pat is illegal, and evasion
// sequences are naturally bounded by the position resolving. [NEEDS SPRT]

// ---- Search statistics (diagnostic) ----
