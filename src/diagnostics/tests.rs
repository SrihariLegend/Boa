use super::*;

pub(in crate::diagnostics) fn test_context() -> (AttackTables, Zobrist) {
    (AttackTables::init(), Zobrist::new())
}

#[test]
pub(in crate::diagnostics) fn startpos_features_are_symmetric_for_raw_mobility() {
    let (atk, z) = test_context();
    let board = Board::startpos();
    let features = extract_restriction_features(&board, &atk, &z, EngineOptions::default());

    assert_eq!(features.mobility_white, 20);
    assert_eq!(features.mobility_black, 20);
    assert_eq!(features.white_pawn_breaks, features.black_pawn_breaks);
    assert_eq!(
        features.piece_redeployment_white,
        features.piece_redeployment_black
    );
}

#[test]
pub(in crate::diagnostics) fn passed_pawn_push_counts_as_pawn_break() {
    let (atk, z) = test_context();
    let board = Board::from_fen("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1").unwrap();
    let features = extract_restriction_features(&board, &atk, &z, EngineOptions::default());

    assert!(features.white_pawn_breaks > 0);
}

#[test]
pub(in crate::diagnostics) fn csv_row_matches_header_width() {
    let (atk, z) = test_context();
    let board = Board::startpos();
    let features = extract_restriction_features(&board, &atk, &z, EngineOptions::default());

    assert_eq!(
        RestrictionFeatures::csv_header().split(',').count(),
        features.to_csv_row().split(',').count()
    );
}
