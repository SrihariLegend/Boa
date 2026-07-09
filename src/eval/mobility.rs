use super::*;
// ============================================================
// Section 4: Mobility and activity
// ============================================================
/// Rook file bonus: open file, semi-open file, or nothing.
pub(in crate::eval) fn rook_file_bonus(file_bb: Bb, our_pawns: Bb, their_pawns: Bb) -> (i32, i32) {
    if our_pawns & file_bb != 0 {
        return (0, 0);
    }
    if their_pawns & file_bb == 0 {
        (ROOK_OPEN_FILE_BONUS.0, ROOK_OPEN_FILE_BONUS.1)
    } else {
        (ROOK_SEMI_OPEN_FILE_BONUS.0, ROOK_SEMI_OPEN_FILE_BONUS.1)
    }
}

pub(in crate::eval) fn mobility_and_activity(board: &Board, ctx: &EvalContext) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;

    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let ci = color as usize;
        let occ = board.occ_all;
        let our_occ = board.occ[ci];

        let their_pawn_attacks = if color == Color::White {
            pawn_attacks_black(board.pieces[1][PieceType::Pawn as usize])
        } else {
            pawn_attacks_white(board.pieces[0][PieceType::Pawn as usize])
        };

        // Knights
        let mut knights = board.pieces[ci][PieceType::Knight as usize];
        while knights != 0 {
            let sq = bb_pop_lsb(&mut knights);
            let atk = ctx.atk.knight[sq as usize];
            let mob = (atk & !our_occ & !their_pawn_attacks).count_ones() as usize;
            let mob = mob.min(8);
            mg += sign * KNIGHT_MOBILITY[mob].0;
            eg += sign * KNIGHT_MOBILITY[mob].1;
            mg += sign * outpost_bonus(sq, color, their_pawn_attacks, board);
        }

        // Bishops
        let mut bishops = board.pieces[ci][PieceType::Bishop as usize];
        while bishops != 0 {
            let sq = bb_pop_lsb(&mut bishops);
            let atk = ctx.atk.bishop_attacks(sq, occ);
            let mob = (atk & !our_occ & !their_pawn_attacks).count_ones() as usize;
            let mob = mob.min(13);
            mg += sign * BISHOP_MOBILITY[mob].0;
            eg += sign * BISHOP_MOBILITY[mob].1;
        }

        // Bishop pair
        if board.pieces[ci][PieceType::Bishop as usize].count_ones() >= 2 {
            mg += sign * BISHOP_PAIR_BONUS.0;
            eg += sign * BISHOP_PAIR_BONUS.1;
        }

        // Rooks
        let mut rooks = board.pieces[ci][PieceType::Rook as usize];
        while rooks != 0 {
            let sq = bb_pop_lsb(&mut rooks);
            let atk = ctx.atk.rook_attacks(sq, occ);
            let mob = (atk & !our_occ & !their_pawn_attacks).count_ones() as usize;
            let mob = mob.min(14);
            mg += sign * ROOK_MOBILITY[mob].0;
            eg += sign * ROOK_MOBILITY[mob].1;

            let file_bb = BB_FILES[sq_file(sq) as usize];
            let our_pawns = board.pieces[ci][PieceType::Pawn as usize];
            let their_pawns = board.pieces[color.flip() as usize][PieceType::Pawn as usize];
            let (rk_mg, rk_eg) = rook_file_bonus(file_bb, our_pawns, their_pawns);
            mg += sign * rk_mg;
            eg += sign * rk_eg;

            let seventh_rank = if color == Color::White {
                BB_RANK_7
            } else {
                BB_RANK_2
            };
            if bb(sq) & seventh_rank != 0 {
                mg += sign * ROOK_ON_SEVENTH_BONUS.0;
                eg += sign * ROOK_ON_SEVENTH_BONUS.1;
            }
        }

        // Queens
        let mut queens = board.pieces[ci][PieceType::Queen as usize];
        while queens != 0 {
            let sq = bb_pop_lsb(&mut queens);
            let atk = ctx.atk.queen_attacks(sq, occ);
            let mob = (atk & !our_occ & !their_pawn_attacks).count_ones() as usize;
            let mob = mob.min(27);
            mg += sign * QUEEN_MOBILITY[mob].0;
            eg += sign * QUEEN_MOBILITY[mob].1;
        }
    }

    (mg, eg)
}

pub(in crate::eval) fn outpost_bonus(
    sq: Square,
    color: Color,
    their_pawn_attacks: Bb,
    board: &Board,
) -> i32 {
    if bb(sq) & their_pawn_attacks != 0 {
        return 0;
    }
    let r = sq_rank(sq);
    let in_outpost_zone = if color == Color::White {
        (3..=5).contains(&r)
    } else {
        (2..=4).contains(&r)
    };
    if !in_outpost_zone {
        return 0;
    }
    let our_pawn_attacks = if color == Color::White {
        pawn_attacks_white(board.pieces[color as usize][PieceType::Pawn as usize])
    } else {
        pawn_attacks_black(board.pieces[color as usize][PieceType::Pawn as usize])
    };
    if our_pawn_attacks & bb(sq) != 0 {
        OUTPOST_SUPPORTED
    } else {
        OUTPOST_UNSUPPORTED
    }
}

// ============================================================
// Section 5: Mobility diagnostics
// ============================================================

/// Total pseudo-legal mobility for one side (pawns incl. pushes/captures, pieces, king).
pub(crate) fn side_mobility(board: &Board, ctx: &EvalContext, color: Color) -> u32 {
    let ci = color as usize;
    let oi = color.flip() as usize;
    let occ = board.occ_all;
    let our_occ = board.occ[ci];

    let mut mobility = 0u32;

    // Pawns: pushes + captures of opponent pieces
    let pawns = board.pieces[ci][PieceType::Pawn as usize];
    if color == Color::White {
        mobility += ((pawns << 8) & !occ).count_ones();
        mobility += (((pawns << 8) & !occ & BB_RANK_3) << 8 & !occ).count_ones();
        mobility += ((pawns << 9) & !BB_FILE_A & board.occ[oi]).count_ones();
        mobility += ((pawns << 7) & !BB_FILE_H & board.occ[oi]).count_ones();
    } else {
        mobility += ((pawns >> 8) & !occ).count_ones();
        mobility += (((pawns >> 8) & !occ & BB_RANK_6) >> 8 & !occ).count_ones();
        mobility += ((pawns >> 7) & !BB_FILE_A & board.occ[oi]).count_ones();
        mobility += ((pawns >> 9) & !BB_FILE_H & board.occ[oi]).count_ones();
    }

    // Knights
    let mut knights = board.pieces[ci][PieceType::Knight as usize];
    while knights != 0 {
        let sq = bb_pop_lsb(&mut knights);
        mobility += (ctx.atk.knight[sq as usize] & !our_occ).count_ones();
    }

    // Bishops
    let mut bishops = board.pieces[ci][PieceType::Bishop as usize];
    while bishops != 0 {
        let sq = bb_pop_lsb(&mut bishops);
        mobility += (ctx.atk.bishop_attacks(sq, occ) & !our_occ).count_ones();
    }

    // Rooks
    let mut rooks = board.pieces[ci][PieceType::Rook as usize];
    while rooks != 0 {
        let sq = bb_pop_lsb(&mut rooks);
        mobility += (ctx.atk.rook_attacks(sq, occ) & !our_occ).count_ones();
    }

    // Queens
    let mut queens = board.pieces[ci][PieceType::Queen as usize];
    while queens != 0 {
        let sq = bb_pop_lsb(&mut queens);
        mobility += (ctx.atk.queen_attacks(sq, occ) & !our_occ).count_ones();
    }

    // King
    let king_sq = board.king_sq[ci];
    if king_sq != NO_SQUARE {
        mobility += (ctx.atk.king[king_sq as usize] & !our_occ).count_ones();
    }

    mobility
}
