use super::super::*;

pub(in crate::search) fn rfp_prune_score(
    static_eval: Score,
    beta: Score,
    depth: i32,
) -> Option<Score> {
    let margin = rfp_margin(depth);
    if depth <= RFP_MAX_DEPTH && static_eval - margin >= beta && !is_mate_score(static_eval) {
        Some(static_eval - margin)
    } else {
        None
    }
}

pub(in crate::search) fn rfp_margin(depth: i32) -> Score {
    RFP_MARGIN_PER_DEPTH * depth
}
