// ---- Search tuning constants ----
// Sources: Stockfish (SF), Chess Programming Wiki (CPW), or marked [NEEDS TUNING].

/// Aspiration window: initial half-width in centipawns.
/// SF uses ~10-20 with gradual widening; 25 is a conservative starting point. [NEEDS TUNING]
pub(in crate::search) const ASPIRATION_DELTA: i32 = 25;

/// Aspiration: use full window below this depth (no point aspirating at low depth).
/// Standard practice: SF uses 4-5.
pub(in crate::search) const ASPIRATION_MIN_DEPTH: u32 = 4;

/// Reverse futility pruning margin per depth unit (centipawns).
/// SF uses ~67-73 (tuned via SPRT). 80 is slightly aggressive. [NEEDS TUNING]
pub(in crate::search) const RFP_MARGIN_PER_DEPTH: i32 = 100;

/// RFP: maximum depth at which to apply.
/// SF applies up to depth ~7-8. 6 is conservative.
pub(in crate::search) const RFP_MAX_DEPTH: i32 = 5;

/// Forward futility pruning: maximum remaining depth.
/// Applied only to quiet, non-checking, non-tactical non-PV moves after legal move filtering.
pub(in crate::search) const FFP_MAX_DEPTH: i32 = 3;

/// Forward futility pruning: base margin multiplied by remaining depth.
/// Probe/tune target. Starts conservative enough to be safe before local sweeps.
pub(in crate::search) const FFP_BASE_MARGIN: i32 = 250;

/// FFP: require quiet move SEE to be non-positive.
pub(in crate::search) const FFP_SEE_GUARD: bool = true;

/// FFP: improving nodes get a larger margin, so pruning is less aggressive.
pub(in crate::search) const FFP_IMPROVING_MULT: f64 = 1.2;

/// FFP: cut nodes may use a smaller margin after counterfactual validation.
pub(in crate::search) const FFP_CUT_MULT: f64 = 1.0;

/// FFP: optional late-move adjustment based on ordered quiet-move index.
pub(in crate::search) const FFP_MOVE_COUNT_THRESHOLD: usize = 10;
pub(in crate::search) const FFP_LATE_MOVE_BONUS: i32 = 0;

/// FFP: quiets with strong positive history are protected.
pub(in crate::search) const FFP_HISTORY_PROTECTION: i32 = 2_048;

/// Null-move pruning: minimum depth to attempt.
/// Standard: 3 (CPW, SF).
pub(in crate::search) const NULL_MOVE_MIN_DEPTH: i32 = 3;

/// Null-move reduction: base + depth/4.
/// SF uses 4 + depth/6 post-tuning; 3 + depth/4 is a common simpler formula (CPW).
pub(in crate::search) const NULL_MOVE_BASE_R: i32 = 3;
pub(in crate::search) const NULL_MOVE_DEPTH_DIVISOR: i32 = 4;

/// Late move reductions: minimum moves searched before applying LMR.
/// Classic conservative LMR: reduce only late quiet moves.
pub(in crate::search) const LMR_FULL_DEPTH_MOVES: usize = 4;

/// LMR: minimum depth to start reducing.
/// Standard: 3 (CPW).
pub(in crate::search) const LMR_MIN_DEPTH: i32 = 3;

/// LMR: log-product divisor. Smaller values reduce more aggressively.
pub(in crate::search) const LMR_LOG_DIVISOR: f64 = 2.5;

/// LMR: normalize quiet history into a small reduction adjustment.
pub(in crate::search) const LMR_HISTORY_CLAMP: i32 = 8_192;
pub(in crate::search) const LMR_HISTORY_NORMALIZER: i32 = 4_096;

/// LMR: extra reduction when the static eval is improving for side to move.
/// Disabled for the learned-criticality baseline; improving remains logged as a feature.
pub(in crate::search) const LMR_IMPROVING_BONUS: i32 = 0;

/// LMR: whether to scale reductions by PV/cut-node type.
pub(in crate::search) const LMR_NODE_TYPE_SCALING: bool = true;

/// Conservative learned-criticality protection for LMR quiets.
///
/// This is the raw full logistic model trained only on shadow-only
/// `counterfactual_probe` rows from the 200-game post-integration dataset at
/// analysis/criticality/2026-06-22_023112418/model-shadow-only.json.
/// We use it only as a ranker: moves at or above the validation P97 score get
/// one ply of reduction protection.  Do not use the calibrated probability for
/// continuous scaling unless calibration improves materially.
pub(in crate::search) const CRITICALITY_P97_LOGIT: f64 = -4.549_676_788_644_19;
pub(in crate::search) const CRITICALITY_INTERCEPT: f64 = -4.531_881_428_371_637;

/// Quiescence delta pruning margin (centipawns).
/// If stand_pat + capture_value + margin < alpha, skip. ~200 is standard (SF, CPW).
pub(in crate::search) const DELTA_PRUNING_MARGIN: i32 = 200;

/// History table overflow threshold — scale down when any entry exceeds this.
/// Prevents history scores from dominating move ordering. [NEEDS TUNING]
pub(in crate::search) const HISTORY_OVERFLOW_THRESHOLD: i32 = 500_000;

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

/// Quiescence check evasion cap. In-check stand-pat is illegal, but unlimited
/// evasion recursion was too expensive; this keeps the tactical fix bounded.
pub(in crate::search) const QS_CHECK_EVASION_MAX_PLY: usize = 2;

// ---- Search statistics (diagnostic) ----
