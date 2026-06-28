use super::super::*;

pub(in crate::search) fn should_ffp_prune(input: FfpInput) -> bool {
    let estimated_gain = ffp_margin(input);
    let required_gain = input.alpha - input.static_eval;

    estimated_gain + FFP_BUFFER < required_gain
}

pub(in crate::search) fn ffp_margin(input: FfpInput) -> i32 {
    let base_gain = FFP_M0 * input.depth;

    let idx = input.move_index.clamp(1, FFP_MAX_RANK) as f64;
    let u_idx = (1.0 - 2.0 * idx.ln() / FFP_IDX_LOG_K.ln()).clamp(-1.0, 1.0);
    let u_node = if input.is_cut_node { -0.6 } else { 0.0 };

    let uncertainty = FFP_W_IDX * u_idx + FFP_W_NODE * u_node;

    // Future extension points: add FFP_W_HIST * u_history and
    // FFP_W_IMPROVING * u_improving terms here when trained.

    let depth_frac = if FFP_MAX_DEPTH > 1 {
        ((input.depth - 1) as f64 / (FFP_MAX_DEPTH - 1) as f64).clamp(0.0, 1.0)
    } else {
        1.0
    };

    (base_gain as f64 + uncertainty * depth_frac).round() as i32
}
