use super::super::*;
use crate::probe;

/// Reverse futility pruning (static null move).
///
/// Returns `Some(score)` if the position is so good that even after the
/// opponent's best efforts over `depth` plies, the score cannot drop below
/// beta — so we can return a lower bound immediately.
///
/// Classical margin: M = RFP_MARGIN_PER_DEPTH × depth
pub(in crate::search) fn rfp_prune_score(
    static_eval: Score,
    beta: Score,
    depth: i32,
) -> Option<Score> {
    let margin = RFP_MARGIN_PER_DEPTH * depth;
    let pruned = depth <= RFP_MAX_DEPTH && static_eval - margin >= beta && !is_mate_score(static_eval);
    probe!(Rfp, RfpEvent {
        depth: depth,
        static_eval: static_eval,
        beta: beta,
        computed_margin: margin,
        pruned: pruned,
    });
    if pruned {
        Some(static_eval - margin)
    } else {
        None
    }
}
