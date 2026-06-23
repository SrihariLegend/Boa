use super::test_utils::*;
use super::*;
#[test]
pub(in crate::search) fn see_scores_clean_capture_at_full_victim_value() {
    assert_eq!(
        see_for("4k3/8/8/8/3q4/8/3R4/4K3 w - - 0 1", "d2d4"),
        PieceType::Queen.material_value()
    );
}

#[test]
pub(in crate::search) fn see_scores_profitable_capture_after_recapture() {
    assert_eq!(
        see_for("r2qk3/8/8/8/8/8/8/3RK3 w - - 0 1", "d1d8"),
        PieceType::Queen.material_value() - PieceType::Rook.material_value()
    );
}

#[test]
pub(in crate::search) fn see_rejects_losing_minor_for_pawn_capture() {
    assert_eq!(
        see_for("4k3/5p2/4p3/8/2B5/8/8/4K3 w - - 0 1", "c4e6"),
        PieceType::Pawn.material_value() - PieceType::Bishop.material_value()
    );
}

#[test]
pub(in crate::search) fn see_scores_even_rook_trade_as_zero() {
    assert_eq!(see_for("r2qk3/8/8/8/8/8/8/R3K3 w - - 0 1", "a1a8"), 0);
}

#[test]
pub(in crate::search) fn see_handles_en_passant_captured_pawn_square() {
    assert_eq!(see_for("3rk3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", "e5d6"), 0);
}

#[test]
pub(in crate::search) fn see_includes_promotion_material_gain() {
    assert_eq!(
        see_for("1r2k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7b8q"),
        PieceType::Rook.material_value() + PieceType::Queen.material_value()
            - PieceType::Pawn.material_value()
    );
}
