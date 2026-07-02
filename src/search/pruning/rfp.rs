use super::super::*;
use crate::probe;

/// Reverse futility pruning (static null move).
///
/// Returns `Some(score)` if the position is so good that even after the
/// opponent's best efforts over `depth` plies, the score cannot drop below
/// beta — so we can return a lower bound immediately.
///
/// Margin: M = RFP_MARGIN_PER_DEPTH × depth + CORR_W_RFP × |corr_val| / 512
/// When correction is large (eval unreliable), margins widen → prune less.
pub(in crate::search) fn rfp_prune_score(
    static_eval: Score,
    beta: Score,
    depth: i32,
    corr_val: i32,
) -> Option<Score> {
    let corr_margin = (corr_w_rfp() * corr_val.abs()) / 512;
    let base_margin = RFP_MARGIN_PER_DEPTH * depth;
    let margin = base_margin + corr_margin;
    let pruned = depth <= RFP_MAX_DEPTH && static_eval - margin >= beta && !is_mate_score(static_eval);

    // Diagnostic: would this decision have been different without correction?
    if corr_margin > 0 && depth <= RFP_MAX_DEPTH && !is_mate_score(static_eval) {
        // rfp_corr_total is on SearchStats, but we don't have access here.
        // We'll count in alpha_beta instead.
    }

    probe!(Rfp, RfpEvent {
        depth: depth,
        static_eval: static_eval,
        beta: beta,
        sigma: 0,
        computed_margin: margin,
        pruned: pruned,
    });
    if pruned {
        Some(static_eval - margin)
    } else {
        None
    }
}
