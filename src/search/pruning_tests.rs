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
pub(in crate::search) fn ffp_margin_uses_history_and_move_index() {
    // At depth=1 with μ=10: base_gain = 10*1 = 10. depth_frac = 0.
    // search_uncertainty*z*depth_frac = 0. sigma_term ≈ 0 (σ=15 at reference).
    // Margin ≈ 10 regardless of move_index/history (zeroed by depth_frac).
    assert_eq!(
        ffp_margin(FfpInput {
            depth: 1,
            static_eval: 0,
            alpha: 0,
            move_index: 1,
            is_cut_node: false,
            history_score: 0,
            sigma: 15,
        }),
        PRUNING_MU  // μ*1 = μ at d=1
    );
    // Even with extreme inputs, depth_frac=0 zeros history/index contribution
    assert_eq!(
        ffp_margin(FfpInput {
            depth: 1,
            static_eval: 0,
            alpha: 0,
            move_index: FFP_MAX_RANK,
            is_cut_node: true,
            history_score: FFP_HISTORY_NORMALIZER,
            sigma: 15,
        }),
        PRUNING_MU  // same — only base_gain contributes at d=1
    );

    // At max depth, early (good) moves get higher margin than late moves
    let early_all = ffp_margin(FfpInput {
        depth: FFP_MAX_DEPTH,
        static_eval: 0,
        alpha: 0,
        move_index: 1,
        is_cut_node: false,
        history_score: 0,
                sigma: 15,
    });
    let late_all = ffp_margin(FfpInput {
        depth: FFP_MAX_DEPTH,
        static_eval: 0,
        alpha: 0,
        move_index: FFP_MAX_RANK,
        is_cut_node: false,
        history_score: 0,
                sigma: 15,
    });
    assert!(early_all > late_all, "early move should have higher margin");

    // High history should increase margin (better δ_m estimate)
    let neutral = ffp_margin(FfpInput {
        depth: FFP_MAX_DEPTH,
        static_eval: 0,
        alpha: 0,
        move_index: 10,
        is_cut_node: false,
        history_score: 0,
                sigma: 15,
    });
    let good_hist = ffp_margin(FfpInput {
        depth: FFP_MAX_DEPTH,
        static_eval: 0,
        alpha: 0,
        move_index: 10,
        is_cut_node: false,
        history_score: FFP_HISTORY_NORMALIZER,
                sigma: 15,
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
                sigma: 15,
    });

    assert!(should_ffp_prune(FfpInput {
        depth: 2,
        static_eval: 0,
        alpha: margin + FFP_BUFFER + 1,
        move_index: 10,
        is_cut_node: false,
        history_score: 0,
                sigma: 15,
    }));

    assert!(!should_ffp_prune(FfpInput {
        depth: 2,
        static_eval: 1,
        alpha: margin + FFP_BUFFER + 1,
        move_index: 10,
        is_cut_node: false,
        history_score: 0,
                sigma: 15,
    }));
}

// ---- Variance estimator tests ----

#[test]
pub(in crate::search) fn sigma_startpos_is_reasonable() {
    let board = Board::startpos();
    let s = sigma(&board);
    // Startpos: all pieces, all files have pawns → open=0, mobile=14, phase≈0
    // Expected: σ_base + w_mob*1 + w_open*0 + w_phase*0 = 10 + 8 = 18
    assert!(s >= 14 && s <= 24, "startpos sigma={} out of [14,24]", s);
}

#[test]
pub(in crate::search) fn sigma_endgame_is_lower() {
    let board = Board::from_fen("8/8/4k3/3p4/3P4/4K3/8/8 w - - 0 1").unwrap();
    let s = sigma(&board);
    assert!(s >= 4 && s <= 18, "endgame sigma={} out of [4,18]", s);
}

#[test]
pub(in crate::search) fn sigma_clamps() {
    let board = Board::startpos();
    let s = sigma(&board);
    assert!(s >= VAR_SIGMA_MIN as i32);
    assert!(s <= VAR_SIGMA_MAX as i32);
}

// ---- RFP variance-aware tests ----

#[test]
pub(in crate::search) fn rfp_margin_grows_with_sigma() {
    let m_low = rfp_margin(3, 8);
    let m_high = rfp_margin(3, 20);
    assert!(m_high > m_low, "higher σ should give larger margin");
}

#[test]
pub(in crate::search) fn rfp_margin_grows_with_depth() {
    let m_shallow = rfp_margin(1, 15);
    let m_deep = rfp_margin(4, 15);
    assert!(m_deep > m_shallow, "deeper search should have larger margin");
}

#[test]
pub(in crate::search) fn rfp_margin_has_correct_structure() {
    // At d=3, σ=15: M = 50*3 + 2.326*15*1.732 = 150 + 60.4 ≈ 210
    let m = rfp_margin(3, 15);
    assert!(m >= 200 && m <= 220, "rfp_margin(3,15)={} out of [200,220]", m);
}
