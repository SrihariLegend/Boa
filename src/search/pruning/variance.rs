use super::super::*;

/// σ(pos): per-ply eval-change standard deviation estimate (centipawns).
///
/// Computed from board features via bit operations — O(1), no allocations,
/// no search state.  Used by both RFP and FFP for variance-aware margins.
///
/// Features (all normalised to [0, 1]):
///   f_mob   = mobile piece count / max (proxy for tactical complexity)
///   f_open  = open files / 8 (penetration pathways for pieces)
///   f_phase = 1 - non_pawn_material / max (endgame discount)
#[inline]
pub fn sigma(board: &Board) -> i32 {
    // ---- f_mob: non-pawn, non-king piece count ----
    let ci_w = Color::White as usize;
    let ci_b = Color::Black as usize;
    let kn = PieceType::Knight as usize;
    let bi = PieceType::Bishop as usize;
    let rk = PieceType::Rook as usize;
    let qu = PieceType::Queen as usize;

    let np_white = board.pieces[ci_w][kn]
        | board.pieces[ci_w][bi]
        | board.pieces[ci_w][rk]
        | board.pieces[ci_w][qu];
    let np_black = board.pieces[ci_b][kn]
        | board.pieces[ci_b][bi]
        | board.pieces[ci_b][rk]
        | board.pieces[ci_b][qu];
    let mobile = bb_popcount(np_white | np_black) as f64;
    let f_mob = (mobile / VAR_MAX_MOBILE).min(1.0);

    // ---- f_open: files with no pawns from either side ----
    let pi = PieceType::Pawn as usize;
    let all_pawns = board.pieces[ci_w][pi] | board.pieces[ci_b][pi];
    let mut open_files: u32 = 0;
    if all_pawns & BB_FILE_A == 0 { open_files += 1; }
    if all_pawns & BB_FILE_B == 0 { open_files += 1; }
    if all_pawns & BB_FILE_C == 0 { open_files += 1; }
    if all_pawns & BB_FILE_D == 0 { open_files += 1; }
    if all_pawns & BB_FILE_E == 0 { open_files += 1; }
    if all_pawns & BB_FILE_F == 0 { open_files += 1; }
    if all_pawns & BB_FILE_G == 0 { open_files += 1; }
    if all_pawns & BB_FILE_H == 0 { open_files += 1; }
    let f_open = open_files as f64 / 8.0;

    // ---- f_phase: 1.0 = opening, 0.0 = endgame ----
    let npm = (board.non_pawn_material(Color::White)
        + board.non_pawn_material(Color::Black)) as f64;
    let f_phase = 1.0 - (npm / VAR_MAX_NON_PAWN_MAT).min(1.0);

    // ---- weighted sum ----
    let raw = VAR_SIGMA_BASE
        + VAR_W_MOBILITY * f_mob
        + VAR_W_OPEN * f_open
        + VAR_W_PHASE * f_phase;

    raw.clamp(VAR_SIGMA_MIN, VAR_SIGMA_MAX).round() as i32
}

// Tests moved to pruning_tests.rs (codebase convention).
