use super::super::*;
use crate::sample_probe;

/// Forward futility pruning: prune quiet moves that cannot reach alpha.
///
/// Classical margin:
///   base_gain = RFP_MARGIN_PER_DEPTH × depth
///   + w_idx × u_idx          // move-ordering position quality
///   + w_hist × u_hist        // history-heuristic quality
///   + CORR_W_FFP × |corr| / 512  // correction uncertainty
///
/// When correction is large (eval unreliable for this position type),
/// the margin widens → less likely to prune → safer.
pub(in crate::search) fn should_ffp_prune(input: FfpInput) -> bool {
    let estimated_gain = ffp_margin(input);
    let required_gain = input.alpha - input.static_eval;
    let pruned = estimated_gain + FFP_BUFFER < required_gain;

    let rate = if input.depth <= 2 { 1 } else { 8 };
    sample_probe!(
        rate,
        Ffp,
        FfpEvent {
            depth: input.depth,
            move_index: input.move_index as u32,
            history_score: input.history_score,
            computed_margin: estimated_gain,
            required_gain: required_gain,
            pruned: pruned,
            is_cut_node: input.is_cut_node,
        }
    );

    pruned
}

pub fn ffp_margin(input: FfpInput) -> i32 {
    let base_gain = RFP_MARGIN_PER_DEPTH * input.depth;

    let idx = (input.move_index.clamp(1, FFP_MAX_RANK)) as f64;
    let u_idx = (1.0 - 2.0 * idx.ln() / (FFP_MAX_RANK as f64).ln()).clamp(-1.0, 1.0);

    let u_hist = (input.history_score as f64 / FFP_HISTORY_NORMALIZER as f64).clamp(-1.0, 1.0);

    let search_uncertainty = FFP_W_IDX * u_idx + FFP_W_HIST * u_hist;

    let depth_frac = if FFP_MAX_DEPTH > 1 {
        ((input.depth - 1) as f64 / (FFP_MAX_DEPTH - 1) as f64).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let corr_term = (corr_w_ffp() * input.corr_val.abs()) / 512;

    (base_gain as f64 + search_uncertainty * depth_frac).round() as i32 + corr_term
}
