use super::*;
// ============================================================
// Section 7: King safety
// ============================================================

pub(in crate::eval) fn king_safety(board: &Board, ctx: &EvalContext) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;

    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let ci = color as usize;
        let ti = color.flip() as usize;
        let king_sq = board.king_sq[ci];
        if king_sq == NO_SQUARE {
            continue;
        }

        // Pawn shield
        let king_file = sq_file(king_sq);
        let shield_rank = if color == Color::White {
            BB_RANK_2 | BB_RANK_3
        } else {
            BB_RANK_6 | BB_RANK_7
        };
        let shield_files = BB_FILES[king_file as usize]
            | (if king_file > 0 {
                BB_FILES[(king_file - 1) as usize]
            } else {
                0
            })
            | (if king_file < 7 {
                BB_FILES[(king_file + 1) as usize]
            } else {
                0
            });
        let shield = board.pieces[ci][PieceType::Pawn as usize] & shield_rank & shield_files;
        let shield_count = shield.count_ones() as i32;
        mg += sign * (shield_count * PAWN_SHIELD_PER_PAWN - PAWN_SHIELD_BASE_PENALTY);

        // King zone attacks
        let king_zone = ctx.atk.king[king_sq as usize] | bb(king_sq);
        let mut attack_units = 0i32;
        let occ = board.occ_all;

        let mut their_knights = board.pieces[ti][PieceType::Knight as usize];
        while their_knights != 0 {
            let sq = bb_pop_lsb(&mut their_knights);
            if ctx.atk.knight[sq as usize] & king_zone != 0 {
                attack_units += KING_ATTACK_WEIGHT_KNIGHT;
            }
        }
        let mut their_bishops = board.pieces[ti][PieceType::Bishop as usize];
        while their_bishops != 0 {
            let sq = bb_pop_lsb(&mut their_bishops);
            if ctx.atk.bishop_attacks(sq, occ) & king_zone != 0 {
                attack_units += KING_ATTACK_WEIGHT_BISHOP;
            }
        }
        let mut their_rooks = board.pieces[ti][PieceType::Rook as usize];
        while their_rooks != 0 {
            let sq = bb_pop_lsb(&mut their_rooks);
            if ctx.atk.rook_attacks(sq, occ) & king_zone != 0 {
                attack_units += KING_ATTACK_WEIGHT_ROOK;
            }
        }
        let mut their_queens = board.pieces[ti][PieceType::Queen as usize];
        while their_queens != 0 {
            let sq = bb_pop_lsb(&mut their_queens);
            if ctx.atk.queen_attacks(sq, occ) & king_zone != 0 {
                attack_units += KING_ATTACK_WEIGHT_QUEEN;
            }
        }

        let penalty = KING_SAFETY_TABLE
            .iter()
            .find(|&&(max_units, _)| attack_units <= max_units)
            .map(|&(_, p)| p)
            .unwrap_or(230);
        mg += sign * (-penalty);

        // King centralization bonus (endgame only)
        let king_rank = sq_rank(king_sq) as i32;
        let center_dist = (3 - king_file as i32)
            .abs()
            .min((4 - king_file as i32).abs())
            + (3 - king_rank).abs().min((4 - king_rank).abs());
        eg += sign * (3 - center_dist).max(0) * KING_CENTRALIZATION_EG;
    }

    (mg, eg)
}

/// Chebyshev (king) distance between two squares
pub(in crate::eval) fn chebyshev_distance(a: Square, b: Square) -> u8 {
    let df = (sq_file(a) as i8 - sq_file(b) as i8).unsigned_abs();
    let dr = (sq_rank(a) as i8 - sq_rank(b) as i8).unsigned_abs();
    df.max(dr)
}
