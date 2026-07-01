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

// ---- Variance-aware futility pruning ----
// Margins are derived from a diffusive model of evaluation evolution:
//   M(d, σ) = μ·d + z·σ·√d
// where μ = expected per-ply improvement, z = confidence z-score,
// σ = position-dependent per-ply eval-change std dev.
//
// Under an approximately diffusive model where eval changes along a line
// have finite variance and weak dependence, uncertainty grows ∝ √d.
// If σ varies across position types (empirically confirmed ~1.5× ratio),
// no fixed linear margin k·d can simultaneously match both distributions —
// a fixed margin is statistically mismatched to at least one regime.
//
// Reference: tools/variance_diagnostic.py and src/bin/variance_diag.rs
// empirically confirm σ ratio ≈ 1.5× between calm and volatile positions.

/// Expected per-ply eval improvement for the side to move (centipawns).
/// This is the μ parameter in M = μ·d + z·σ·√d.
/// Empirically ~5-15 cp per ply. 10 is a conservative initial value. [NEEDS SPRT]
pub(in crate::search) const PRUNING_MU: i32 = 50;

/// Confidence z-score disabled — variance-aware RFP margin moved to research.
/// See EXPERIMENTS.md § "Variance-Aware Futility Pruning".
pub(in crate::search) const PRUNING_Z: f64 = 0.0;

/// sqrt(d) lookup for d in [0, RFP_MAX_DEPTH].
/// Avoids floating-point sqrt in the pruning hot path.
pub(in crate::search) const SQRT_D: [f64; 8] = [
    0.0, 1.0, 1.414_213_562, 1.732_050_808, 2.0, 2.236_067_977, 2.449_489_743, 2.645_751_311,
];

// ---- Position variance estimator σ(pos) ----
// σ ∈ [SIGMA_MIN, SIGMA_MAX], computed from board features via bit operations.
// Features are normalized to [0, 1] to keep coefficients independent of scale.

/// Baseline per-ply std dev (centipawns). Calm midgame floor.
pub(in crate::search) const VAR_SIGMA_BASE: f64 = 10.0;

/// Mobility coefficient: contribution at maximum mobile-piece count.
pub(in crate::search) const VAR_W_MOBILITY: f64 = 8.0;

/// Open-files coefficient: contribution when all 8 files are open.
pub(in crate::search) const VAR_W_OPEN: f64 = 6.0;

/// Phase coefficient: endgame discount (negative = endgames are calmer).
pub(in crate::search) const VAR_W_PHASE: f64 = -4.0;

/// σ clamp range (centipawns).
pub(in crate::search) const VAR_SIGMA_MIN: f64 = 6.0;
pub(in crate::search) const VAR_SIGMA_MAX: f64 = 24.0;

/// Maximum non-pawn, non-king piece count for mobility normalization.
/// Queen = 1 piece (not 9). 14 = 7 each side in the opening.
pub(in crate::search) const VAR_MAX_MOBILE: f64 = 14.0;

/// Maximum non-pawn material for phase normalization (centipawns).
/// Both sides with 2N+2B+2R+1Q = 2×(640+660+1000+900) = 6400 cp.
pub(in crate::search) const VAR_MAX_NON_PAWN_MAT: f64 = 6400.0;

// ---- RFP ----

/// RFP: maximum depth at which to apply.
pub(in crate::search) const RFP_MAX_DEPTH: i32 = 5;

// ---- FFP ----

/// FFP: maximum remaining depth.
pub(in crate::search) const FFP_MAX_DEPTH: i32 = 4;

/// FFP: move-index uncertainty weight (centipawns). Kept from the classical
/// criticality-guided FFP; retune if the history term is added.
pub(in crate::search) const FFP_W_IDX: f64 = 40.0;

/// FFP: history-score coefficient (centipawns). High-history moves are more
/// likely to improve the eval and should have a higher estimated gain.
/// Normalized so max history ≈ this many extra centipawns of expected gain.
pub(in crate::search) const FFP_W_HIST: f64 = 20.0;

/// FFP: history score normalizer. History scores are divided by this before
/// clamping to [-1, 1]. 32_768 = 2 × HISTORY_GRAVITY.
pub(in crate::search) const FFP_HISTORY_NORMALIZER: i32 = 32_768;

/// FFP: σ reference value for normalisation (centipawns).
/// σ values are normalised as (σ / FFP_SIGMA_REF − 1), clamped to [−1, 1].
pub(in crate::search) const FFP_SIGMA_REF: f64 = 15.0;

/// FFP σ term disabled — variance-aware FFP moved to research.
/// See EXPERIMENTS.md § "Variance-Aware Futility Pruning".
pub(in crate::search) const FFP_W_SIGMA: f64 = 0.0;

/// FFP: safety buffer added to the estimated gain (centipawns).
pub(in crate::search) const FFP_BUFFER: i32 = 0;

/// FFP: maximum move rank for log-scaled move-index uncertainty.
pub(in crate::search) const FFP_MAX_RANK: usize = 60;

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
/// Clamped at HISTORY_GRAVITY (the natural ceiling under gravity aging).
pub(in crate::search) const LMR_HISTORY_CLAMP: i32 = HISTORY_GRAVITY;
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
