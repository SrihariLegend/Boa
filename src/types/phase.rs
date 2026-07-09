// ---- Game phase ----
// Linear interpolation between midgame and endgame evaluation

/// Total non-pawn material when all pieces are on the board:
/// 2 × (2×Knight=640 + 2×Bishop=660 + 2×Rook=1000 + Queen=900) = 6400
/// We use this as the denominator for phase interpolation.
pub(super) const TOTAL_NON_PAWN_MATERIAL: i32 = 2 * (2 * 320 + 2 * 330 + 2 * 500 + 900);

pub fn game_phase(non_pawn_material: i32) -> i32 {
    (non_pawn_material * 256 / TOTAL_NON_PAWN_MATERIAL).min(256)
}
