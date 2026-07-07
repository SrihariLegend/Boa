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
pub(in crate::search) fn lmr_improving_adds_bonus_reduction() {
    let mut input = reducible_lmr_input(8, LMR_FULL_DEPTH_MOVES + 4);
    let base = lmr_reduction_for(input);
    input.improving = true;
    assert_eq!(lmr_reduction_for(input), base - 1);
}

#[test]
pub(in crate::search) fn lmr_uses_history_to_adjust_reduction() {
    let mut good_history = reducible_lmr_input(12, LMR_FULL_DEPTH_MOVES + 16);
    good_history.history_score = LMR_HISTORY_NORMALIZER * 4;
    let mut bad_history = good_history;
    bad_history.history_score = -LMR_HISTORY_NORMALIZER * 4;

    let neutral_history = lmr_reduction_for(reducible_lmr_input(12, LMR_FULL_DEPTH_MOVES + 16));
    assert_eq!(lmr_reduction_for(bad_history), neutral_history + 4);
    assert_eq!(lmr_reduction_for(good_history), (neutral_history - 4).max(0));
}

// ---- FFP tests ----

#[test]
pub(in crate::search) fn ffp_margin_uses_history_and_move_index() {
    assert_eq!(
        ffp_margin(FfpInput {
            depth: 1,
            static_eval: 0,
            alpha: 0,
            move_index: 1,
            is_cut_node: false,
            history_score: 0,
            corr_val: 0,
        }),
        RFP_MARGIN_PER_DEPTH
    );
    assert_eq!(
        ffp_margin(FfpInput {
            depth: 1,
            static_eval: 0,
            alpha: 0,
            move_index: FFP_MAX_RANK,
            is_cut_node: true,
            history_score: FFP_HISTORY_NORMALIZER,
            corr_val: 0,
        }),
        RFP_MARGIN_PER_DEPTH
    );

    let early_all = ffp_margin(FfpInput {
        depth: FFP_MAX_DEPTH,
        static_eval: 0,
        alpha: 0,
        move_index: 1,
        is_cut_node: false,
        history_score: 0,
        corr_val: 0,
    });
    let late_all = ffp_margin(FfpInput {
        depth: FFP_MAX_DEPTH,
        static_eval: 0,
        alpha: 0,
        move_index: FFP_MAX_RANK,
        is_cut_node: false,
        history_score: 0,
        corr_val: 0,
    });
    assert!(early_all > late_all, "early move should have higher margin");

    let neutral = ffp_margin(FfpInput {
        depth: FFP_MAX_DEPTH,
        static_eval: 0,
        alpha: 0,
        move_index: 10,
        is_cut_node: false,
        history_score: 0,
        corr_val: 0,
    });
    let good_hist = ffp_margin(FfpInput {
        depth: FFP_MAX_DEPTH,
        static_eval: 0,
        alpha: 0,
        move_index: 10,
        is_cut_node: false,
        history_score: FFP_HISTORY_NORMALIZER,
        corr_val: 0,
    });
    assert!(good_hist > neutral, "good history should increase margin");
}

#[test]
pub(in crate::search) fn ffp_prunes_only_beyond_safety_buffer() {
    let margin = ffp_margin(FfpInput {
        depth: 2,
        static_eval: 0,
        alpha: 0,
        move_index: 10,
        is_cut_node: false,
        history_score: 0,
        corr_val: 0,
    });

    assert!(should_ffp_prune(FfpInput {
        depth: 2,
        static_eval: 0,
        alpha: margin + FFP_BUFFER + 1,
        move_index: 10,
        is_cut_node: false,
        history_score: 0,
        corr_val: 0,
    }));

    assert!(!should_ffp_prune(FfpInput {
        depth: 2,
        static_eval: 1,
        alpha: margin + FFP_BUFFER + 1,
        move_index: 10,
        is_cut_node: false,
        history_score: 0,
        corr_val: 0,
    }));
}

// ---- RFP classical tests ----

#[test]
pub(in crate::search) fn rfp_margin_linear_with_depth() {
    let m1 = RFP_MARGIN_PER_DEPTH * 1;
    let m4 = RFP_MARGIN_PER_DEPTH * 4;
    assert!(m4 > m1, "deeper search should have larger margin");
}

#[test]
pub(in crate::search) fn rfp_prunes_when_eval_well_above_beta() {
    // At depth 3, margin = 150. eval=200, beta=0: 200-150=50 >= 0 → prune
    assert!(rfp_prune_score(200, 0, 3, 0).is_some());
    // At depth 3, margin = 150. eval=100, beta=0: 100-150=-50 < 0 → don't prune
    assert!(rfp_prune_score(100, 0, 3, 0).is_none());
}
