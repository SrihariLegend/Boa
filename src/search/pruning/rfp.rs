use super::super::*;

/// Reverse futility pruning (static null move).
///
/// Returns `Some(score)` if the position is so good that even after the
/// opponent's best efforts over `depth` plies, the score cannot drop below
/// beta — so we can return a lower bound immediately.
///
/// Margin formula (variance-aware):  M = μ·d + z·σ·√d
///
///   μ  = expected per-ply improvement for the side to move
///   z  = confidence z-score (one-sided bound)
///   σ  = position-dependent per-ply std dev of eval changes
///   √d = square-root of depth (looked up from SQRT_D table)
pub(in crate::search) fn rfp_prune_score(
    static_eval: Score,
    beta: Score,
    depth: i32,
    sigma: i32,
) -> Option<Score> {
    let margin = rfp_margin(depth, sigma);
    if depth <= RFP_MAX_DEPTH && static_eval - margin >= beta && !is_mate_score(static_eval) {
        Some(static_eval - margin)
    } else {
        None
    }
}

#[inline]
pub fn rfp_margin(depth: i32, sigma: i32) -> Score {
    // M = μ·d + z·σ·√d
    let mean_term = PRUNING_MU * depth;
    let sqrt_d = SQRT_D[(depth as usize).min(7)];
    let uncertainty = (PRUNING_Z * sigma as f64 * sqrt_d).round() as i32;
    mean_term + uncertainty
}
