use super::super::*;

pub(in crate::search) fn should_ffp_prune(input: FfpInput) -> bool {
    let margin = ffp_margin(input);

    if input.static_eval.saturating_add(margin) > input.alpha {
        return false;
    }
    if FFP_SEE_GUARD && input.see > 0 {
        return false;
    }
    true
}

pub(in crate::search) fn ffp_margin(input: FfpInput) -> i32 {
    let mut margin = FFP_BASE_MARGIN * input.depth;

    if input.improving {
        margin = (margin as f64 * FFP_IMPROVING_MULT) as i32;
    }
    if input.is_cut_node {
        margin = (margin as f64 * FFP_CUT_MULT) as i32;
    }
    if input.move_count > FFP_MOVE_COUNT_THRESHOLD {
        margin += FFP_LATE_MOVE_BONUS;
    }

    margin
}
