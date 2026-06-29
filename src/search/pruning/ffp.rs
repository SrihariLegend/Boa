use super::super::*;

/// Forward futility pruning: prune quiet moves that cannot reach alpha.
///
/// Decision: can this quiet move (plus the opponent's d-1 plies of effort)
/// plausibly raise the eval from static_eval to alpha?
///
/// Margin estimates the expected gain from searching this move:
///   base_gain = μ · (d-1)     // opponent's expected improvement
///   + w_idx · u_idx          // move-ordering position quality
///   + w_hist · u_hist        // history-heuristic quality (δ_m estimate)
///
/// scaled by depth fraction.  If estimated_gain + buffer < required_gain,
/// the move is unlikely to matter → prune.
pub(in crate::search) fn should_ffp_prune(input: FfpInput) -> bool {
    let estimated_gain = ffp_margin(input);
    let required_gain = input.alpha - input.static_eval;
    estimated_gain + FFP_BUFFER < required_gain
}

pub fn ffp_margin(input: FfpInput) -> i32 {
    // Expected total improvement over the search depth.
    // Uses μ*d (not μ*(d−1)) so the margin has a non-zero floor at d=1
    // — our move itself contributes gain even when the opponent has 0 plies.
    let base_gain = PRUNING_MU * input.depth;

    // Move-index uncertainty (log-scaled): earlier moves are more likely good
    let idx = (input.move_index.clamp(1, FFP_MAX_RANK)) as f64;
    let u_idx = (1.0 - 2.0 * idx.ln() / (FFP_MAX_RANK as f64).ln()).clamp(-1.0, 1.0);

    // History-based δ_m estimate: historically good moves have higher gain
    let u_hist = (input.history_score as f64 / FFP_HISTORY_NORMALIZER as f64).clamp(-1.0, 1.0);

    // σ-based caution: volatile positions have higher decision risk.
    // σ is position volatility — it does NOT scale with depth (unlike
    // move-index and history uncertainty, which depend on remaining plies).
    // We apply sigma_term directly without the depth_frac multiplier so it
    // remains active even at depth 1 where depth_frac = 0.
    let u_sigma = (input.sigma as f64 / FFP_SIGMA_REF - 1.0).clamp(-1.0, 1.0);
    let sigma_term = FFP_W_SIGMA * u_sigma;

    // Search-depth-scaled uncertainty: move-index and history quality are
    // about how much the remaining search plies will improve the eval.
    let search_uncertainty = FFP_W_IDX * u_idx + FFP_W_HIST * u_hist;

    let depth_frac = if FFP_MAX_DEPTH > 1 {
        ((input.depth - 1) as f64 / (FFP_MAX_DEPTH - 1) as f64).clamp(0.0, 1.0)
    } else {
        1.0
    };

    (base_gain as f64 + search_uncertainty * depth_frac + sigma_term).round() as i32
}
