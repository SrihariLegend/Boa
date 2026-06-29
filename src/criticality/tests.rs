use super::*;
use crate::types::{make_move, Color};

#[test]
pub(super) fn sampler_respects_extreme_permille_values() {
    let m = make_move(12, 28);
    assert!(!should_probe(1, m, 8, 3, 4, 0));
    assert!(should_probe(1, m, 8, 3, 4, 1000));
}

#[test]
pub(super) fn sampler_is_deterministic() {
    let m = make_move(12, 28);
    assert_eq!(
        criticality_sample_bucket(123, m, 9, 4, 55),
        criticality_sample_bucket(123, m, 9, 4, 55)
    );
}

#[test]
pub(super) fn record_row_matches_header_width() {
    let m = make_move(12, 28);
    let record = CriticalityRecord {
        decision_kind: CriticalityDecisionKind::Lmr,
        pid: 1,
        game_id: 2,
        search_id: 3,
        root_depth: 6,
        ply: 2,
        node_hash: 123,
        side_to_move: Color::White,
        m,
        from: 12,
        to: 28,
        piece: crate::types::make_piece(Color::White, crate::types::PieceType::Pawn),
        depth: 5,
        move_index: 7,
        base_reduction: 2,
        final_reduction: 1,
        new_depth: 3,
        history_score: 42,
        static_eval: 10,
        prev_static_eval: Some(-5),
        alpha: -20,
        beta: 30,
        futility_margin: None,
        static_alpha_margin: None,
        is_pv: false,
        is_cut_node: true,
        improving: true,
        is_killer: false,
        is_counter: false,
        tt_move_agreement: false,
        label_source: CriticalityLabelSource::CounterfactualProbe,
        reduced_score: Some(-10),
        full_score: Some(35),
        sigma: Some(15),
    };
    assert_eq!(
        CriticalityRecord::header().split(',').count(),
        record.to_csv_row().split(',').count()
    );
}
