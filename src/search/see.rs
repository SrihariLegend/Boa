use super::*;
use crate::sample_probe;

pub(in crate::search) fn static_exchange_eval(board: &Board, atk: &AttackTables, m: Move) -> i32 {
    if m == MOVE_NONE {
        return 0;
    }

    let from = move_from(m);
    let to = move_to(m);
    let flags = move_flags(m);
    let mover = board.sq_piece[from as usize];
    if mover == PIECE_NONE {
        return 0;
    }

    let moving_side = board.side;
    let mover_type = piece_type(mover);
    let moved_type = if flags == MF_PROMOTION {
        move_promo_pt(m)
    } else {
        mover_type
    };
    let captured_value = captured_value_for_see(board, m);
    let promotion_gain = if flags == MF_PROMOTION {
        moved_type.material_value() - PieceType::Pawn.material_value()
    } else {
        0
    };

    let mut pieces = board.pieces;
    let mut occ = board.occ_all;

    pieces[moving_side as usize][mover_type as usize] &= !bb(from);
    occ &= !bb(from);

    if flags == MF_EN_PASSANT {
        let cap_sq = if moving_side == Color::White {
            to - 8
        } else {
            to + 8
        };
        pieces[moving_side.flip() as usize][PieceType::Pawn as usize] &= !bb(cap_sq);
        occ &= !bb(cap_sq);
    } else {
        let captured = board.sq_piece[to as usize];
        if captured != PIECE_NONE {
            pieces[piece_color(captured) as usize][piece_type(captured) as usize] &= !bb(to);
        }
        occ &= !bb(to);
    }

    pieces[moving_side as usize][moved_type as usize] |= bb(to);
    occ |= bb(to);

    let mut gain = [0i32; 32];
    gain[0] = captured_value + promotion_gain;

    let mut depth = 0usize;
    let mut side = moving_side.flip();
    let mut victim_side = moving_side;
    let mut victim_type = moved_type;
    let mut victim_value = moved_type.material_value();
    let target_bb = bb(to);
    while depth + 1 < gain.len() {
        let Some((attacker_sq, attacker_type)) =
            least_valuable_attacker(
                to, side, occ, &pieces, atk,
                board.king_sq[side as usize],
            )
        else {
            break;
        };

        let attacker_bb = bb(attacker_sq);
        pieces[victim_side as usize][victim_type as usize] &= !target_bb;
        pieces[side as usize][attacker_type as usize] &= !attacker_bb;
        pieces[side as usize][attacker_type as usize] |= target_bb;
        occ &= !attacker_bb;

        if attacker_type == PieceType::King
            && attackers_to(to, side.flip(), occ, &pieces, atk)
                & color_occupancy(&pieces, side.flip())
                != 0
        {
            break;
        }

        depth += 1;
        gain[depth] = victim_value - gain[depth - 1];
        victim_side = side;
        victim_type = attacker_type;
        victim_value = attacker_type.material_value();
        side = side.flip();
    }

    while depth > 0 {
        depth -= 1;
        gain[depth] = -gain[depth + 1].max(-gain[depth]);
    }

    gain[0]
}

pub(in crate::search) fn captured_value_for_see(board: &Board, m: Move) -> i32 {
    if move_flags(m) == MF_EN_PASSANT {
        return PieceType::Pawn.material_value();
    }
    let captured = board.sq_piece[move_to(m) as usize];
    if captured == PIECE_NONE {
        return 0;
    }
    piece_type(captured).material_value()
}

pub(in crate::search) fn least_valuable_attacker(
    target: Square,
    color: Color,
    occ: Bb,
    pieces: &[[Bb; 6]; 2],
    atk: &AttackTables,
    king_sq: Square,
) -> Option<(Square, PieceType)> {
    let attackers = attackers_to(target, color, occ, pieces, atk);
    if attackers == 0 {
        return None;
    }

    let ci = color as usize;
    for pt in [
        PieceType::Pawn,
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ] {
        let mut bb = attackers & pieces[ci][pt as usize];
        if bb == 0 {
            continue;
        }

        // Fast path: first attacker is usually not pinned.
        // Only enter the slow loop if the first is actually pinned.
        let first_sq = bb_lsb(bb);
        if pt == PieceType::King
            || !is_pinned(king_sq, first_sq, color, occ, pieces, atk, target)
        {
            return Some((first_sq, pt));
        }

        // Slow path: first attacker was pinned — scan remaining.
        bb &= bb - 1;
        while bb != 0 {
            let sq = bb_lsb(bb);
            bb &= bb - 1;
            if !is_pinned(king_sq, sq, color, occ, pieces, atk, target) {
                return Some((sq, pt));
            }
        }
    }

    None
}

/// Check whether the piece at `sq` is absolutely pinned to its king
/// such that it cannot legally move to `target_sq`.
///
/// A piece is absolutely pinned if removing it would reveal an enemy
/// sliding piece (rook/queen on rank/file, bishop/queen on diagonal)
/// attacking its king. However, if the piece is moving to capture the
/// pinning piece (it moves onto the pinner's square), the pin is released
/// — the capture removes the threat.
#[inline(always)]
pub(in crate::search) fn is_pinned(
    king_sq: Square,
    sq: Square,
    color: Color,
    occ: Bb,
    pieces: &[[Bb; 6]; 2],
    atk: &AttackTables,
    target_sq: Square,
) -> bool {
    // Fast early exit (>99% of calls): piece must share a line with its king.
    let kf = sq_file(king_sq) as i8;
    let kr = sq_rank(king_sq) as i8;
    let sf = sq_file(sq) as i8;
    let sr = sq_rank(sq) as i8;
    let same_rank = kr == sr;
    let same_file = kf == sf;
    let same_diag = (kf - sf).abs() == (kr - sr).abs();
    if !same_rank && !same_file && !same_diag {
        return false;
    }

    // If moving along the same ray toward the pinner or capturing the pinner,
    // the piece stays on the pin line — not pinned.
    let tf = sq_file(target_sq) as i8;
    let tr = sq_rank(target_sq) as i8;
    if (same_rank && kr == tr)
        || (same_file && kf == tf)
        || (same_diag && (kf - sf).abs() == (kr - sr).abs()
            && (kf - tf).abs() == (kr - tr).abs()
            && (kf - sf).signum() == (kf - tf).signum())
    {
        return false;
    }

    // Only now do the expensive magic bitboard lookups (<1% of calls).
    let enemy = color.flip();
    let enemy_idx = enemy as usize;
    let occ_without = occ & !bb(sq);
    let sliders = (atk.rook_attacks(king_sq, occ_without)
        & (pieces[enemy_idx][PieceType::Rook as usize]
            | pieces[enemy_idx][PieceType::Queen as usize]))
        | (atk.bishop_attacks(king_sq, occ_without)
            & (pieces[enemy_idx][PieceType::Bishop as usize]
                | pieces[enemy_idx][PieceType::Queen as usize]));

    sliders != 0 && (sliders & bb(target_sq)) == 0
}

pub(in crate::search) fn attackers_to(
    target: Square,
    color: Color,
    occ: Bb,
    pieces: &[[Bb; 6]; 2],
    atk: &AttackTables,
) -> Bb {
    let ci = color as usize;
    let target_bb = bb(target);
    let pawn_attackers = if color == Color::White {
        crate::movegen::pawn_attacks_black(target_bb)
    } else {
        crate::movegen::pawn_attacks_white(target_bb)
    } & pieces[ci][PieceType::Pawn as usize];
    let knight_attackers = atk.knight[target as usize] & pieces[ci][PieceType::Knight as usize];
    let bishop_attackers = atk.bishop_attacks(target, occ)
        & (pieces[ci][PieceType::Bishop as usize] | pieces[ci][PieceType::Queen as usize]);
    let rook_attackers = atk.rook_attacks(target, occ)
        & (pieces[ci][PieceType::Rook as usize] | pieces[ci][PieceType::Queen as usize]);
    let king_attackers = atk.king[target as usize] & pieces[ci][PieceType::King as usize];

    pawn_attackers | knight_attackers | bishop_attackers | rook_attackers | king_attackers
}

pub(in crate::search) fn color_occupancy(pieces: &[[Bb; 6]; 2], color: Color) -> Bb {
    pieces[color as usize].iter().fold(0, |occ, bb| occ | *bb)
}

// ============================================================
// Section 8: Draw detection helpers
// ============================================================

pub(in crate::search) fn is_insufficient_material(board: &Board) -> bool {
    if board.occ_all.count_ones() == 2 {
        return true;
    }
    if board.occ_all.count_ones() == 3 {
        let bishops = board.pieces[0][PieceType::Bishop as usize]
            | board.pieces[1][PieceType::Bishop as usize];
        let knights = board.pieces[0][PieceType::Knight as usize]
            | board.pieces[1][PieceType::Knight as usize];
        if (bishops | knights).count_ones() == 1 {
            return true;
        }
    }
    false
}
