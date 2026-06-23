use super::test_utils::*;
use super::*;
#[test]
pub(in crate::search) fn lmr_keeps_protected_moves_full_depth() {
    let mut input = reducible_lmr_input(8, LMR_FULL_DEPTH_MOVES);
    input.is_capture = true;
    assert_eq!(lmr_reduction_for(input), 0);

    let mut input = reducible_lmr_input(8, LMR_FULL_DEPTH_MOVES);
    input.gives_check = true;
    assert_eq!(lmr_reduction_for(input), 0);
}

#[test]
pub(in crate::search) fn lmr_scales_with_depth_and_move_count() {
    let shallow = lmr_reduction_for(reducible_lmr_input(5, LMR_FULL_DEPTH_MOVES + 3));
    let deep_late = lmr_reduction_for(reducible_lmr_input(12, LMR_FULL_DEPTH_MOVES + 16));
    assert!(deep_late > shallow);
}

#[test]
pub(in crate::search) fn lmr_base_formula_rounds_to_nearest() {
    assert_eq!(
        lmr_reduction_details_for(reducible_lmr_input(3, LMR_FULL_DEPTH_MOVES + 3)).base_reduction,
        1
    );
}

#[test]
pub(in crate::search) fn lmr_improving_is_logged_but_does_not_change_reduction() {
    let mut input = reducible_lmr_input(8, LMR_FULL_DEPTH_MOVES);
    assert_eq!(lmr_reduction_for(input), 0);
    input.improving = true;
    assert_eq!(lmr_reduction_for(input), 0);
}

#[test]
pub(in crate::search) fn lmr_uses_history_to_adjust_reduction() {
    let mut good_history = reducible_lmr_input(12, LMR_FULL_DEPTH_MOVES + 16);
    good_history.history_score = LMR_HISTORY_CLAMP;
    let mut bad_history = good_history;
    bad_history.history_score = -LMR_HISTORY_CLAMP;

    let neutral_history = lmr_reduction_for(reducible_lmr_input(12, LMR_FULL_DEPTH_MOVES + 16));
    assert_eq!(lmr_reduction_for(bad_history), neutral_history);
    assert!(lmr_reduction_for(good_history) < neutral_history);
}

#[test]
pub(in crate::search) fn lmr_applies_learned_criticality_p97_protection() {
    let baseline = reducible_lmr_input(12, LMR_FULL_DEPTH_MOVES + 16);
    let baseline_reduction = lmr_reduction_for(baseline);
    assert!(baseline_reduction > 0);

    let mut critical = baseline;
    critical.ply = 20;
    critical.static_eval = 2_000;
    critical.prev_static_eval = Some(-2_000);
    critical.alpha = -2_000;
    critical.beta = -1_900;
    critical.is_counter = true;

    assert!(
        criticality_score(critical, baseline_reduction, baseline_reduction)
            >= CRITICALITY_P97_LOGIT
    );
    assert_eq!(lmr_reduction_for(critical), baseline_reduction - 1);
}

#[test]
pub(in crate::search) fn ffp_uses_criticality_guided_margin() {
    assert_eq!(
        ffp_margin(FfpInput {
            depth: 1,
            static_eval: 0,
            alpha: 0,
            move_index: 1,
            is_cut_node: false,
        }),
        FFP_M0
    );

    let early_all = ffp_margin(FfpInput {
        depth: FFP_MAX_DEPTH,
        static_eval: 0,
        alpha: 0,
        move_index: 1,
        is_cut_node: false,
    });
    let late_all = ffp_margin(FfpInput {
        depth: FFP_MAX_DEPTH,
        static_eval: 0,
        alpha: 0,
        move_index: FFP_MAX_RANK,
        is_cut_node: false,
    });
    let late_cut = ffp_margin(FfpInput {
        depth: FFP_MAX_DEPTH,
        static_eval: 0,
        alpha: 0,
        move_index: FFP_MAX_RANK,
        is_cut_node: true,
    });

    assert!(early_all > late_all);
    assert!(late_cut < late_all);
}

#[test]
pub(in crate::search) fn ffp_prunes_only_beyond_safety_buffer() {
    let margin = ffp_margin(FfpInput {
        depth: 2,
        static_eval: 0,
        alpha: 0,
        move_index: 10,
        is_cut_node: false,
    });

    assert!(should_ffp_prune(FfpInput {
        depth: 2,
        static_eval: 0,
        alpha: margin + FFP_BUFFER + 1,
        move_index: 10,
        is_cut_node: false,
    }));

    assert!(!should_ffp_prune(FfpInput {
        depth: 2,
        static_eval: 1,
        alpha: margin + FFP_BUFFER + 1,
        move_index: 10,
        is_cut_node: false,
    }));
}
