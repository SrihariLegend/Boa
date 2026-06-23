use super::*;
// ============================================================
// Section 4: Move generation
// ============================================================

/// Generate all pseudo-legal moves for the side to move.
pub fn gen_moves(board: &Board, atk: &AttackTables) -> MoveList {
    let mut list = MoveList::new();
    let us = board.side;
    let them = us.flip();
    let ui = us as usize;
    let ti = them as usize;
    let occ = board.occ_all;
    let our = board.occ[ui];
    let their = board.occ[ti];

    // ---- Pawns ----
    let pawns = board.pieces[ui][PieceType::Pawn as usize];
    if us == Color::White {
        gen_pawn_moves_white(pawns, their, board.ep_sq, occ, &mut list);
    } else {
        gen_pawn_moves_black(pawns, their, board.ep_sq, occ, &mut list);
    }

    // ---- Knights ----
    let mut knights = board.pieces[ui][PieceType::Knight as usize];
    while knights != 0 {
        let from = bb_pop_lsb(&mut knights);
        let targets = atk.knight[from as usize] & !our;
        add_moves(from, targets, &mut list);
    }

    // ---- Bishops ----
    let mut bishops = board.pieces[ui][PieceType::Bishop as usize];
    while bishops != 0 {
        let from = bb_pop_lsb(&mut bishops);
        let targets = atk.bishop_attacks(from, occ) & !our;
        add_moves(from, targets, &mut list);
    }

    // ---- Rooks ----
    let mut rooks = board.pieces[ui][PieceType::Rook as usize];
    while rooks != 0 {
        let from = bb_pop_lsb(&mut rooks);
        let targets = atk.rook_attacks(from, occ) & !our;
        add_moves(from, targets, &mut list);
    }

    // ---- Queens ----
    let mut queens = board.pieces[ui][PieceType::Queen as usize];
    while queens != 0 {
        let from = bb_pop_lsb(&mut queens);
        let targets = atk.queen_attacks(from, occ) & !our;
        add_moves(from, targets, &mut list);
    }

    // ---- King ----
    let king_sq = board.king_sq[ui];
    if king_sq != NO_SQUARE {
        let targets = atk.king[king_sq as usize] & !our;
        add_moves(king_sq, targets, &mut list);
        gen_castling(board, king_sq, us, occ, &mut list);
    }

    list
}

/// Generate only captures and promotions (for quiescence search)
pub fn gen_captures(board: &Board, atk: &AttackTables) -> MoveList {
    let mut list = MoveList::new();
    let us = board.side;
    let them = us.flip();
    let ui = us as usize;
    let ti = them as usize;
    let occ = board.occ_all;
    let their = board.occ[ti];

    // Pawn captures + promotions
    let pawns = board.pieces[ui][PieceType::Pawn as usize];
    if us == Color::White {
        gen_pawn_captures_white(pawns, their, board.ep_sq, occ, &mut list);
    } else {
        gen_pawn_captures_black(pawns, their, board.ep_sq, occ, &mut list);
    }

    // Knight captures
    let mut knights = board.pieces[ui][PieceType::Knight as usize];
    while knights != 0 {
        let from = bb_pop_lsb(&mut knights);
        add_moves(from, atk.knight[from as usize] & their, &mut list);
    }

    // Bishop captures
    let mut bishops = board.pieces[ui][PieceType::Bishop as usize];
    while bishops != 0 {
        let from = bb_pop_lsb(&mut bishops);
        add_moves(from, atk.bishop_attacks(from, occ) & their, &mut list);
    }

    // Rook captures
    let mut rooks = board.pieces[ui][PieceType::Rook as usize];
    while rooks != 0 {
        let from = bb_pop_lsb(&mut rooks);
        add_moves(from, atk.rook_attacks(from, occ) & their, &mut list);
    }

    // Queen captures
    let mut queens = board.pieces[ui][PieceType::Queen as usize];
    while queens != 0 {
        let from = bb_pop_lsb(&mut queens);
        add_moves(from, atk.queen_attacks(from, occ) & their, &mut list);
    }

    // King captures
    let king_sq = board.king_sq[ui];
    if king_sq != NO_SQUARE {
        add_moves(king_sq, atk.king[king_sq as usize] & their, &mut list);
    }

    list
}

// ---- Pawn move helpers ----

pub(super) fn gen_pawn_moves_white(
    pawns: Bb,
    their: Bb,
    ep_sq: Square,
    occ: Bb,
    list: &mut MoveList,
) {
    let single = (pawns << 8) & !occ;
    let promo = single & BB_RANK_8;
    let normal = single & !BB_RANK_8;
    add_pawn_moves(normal, -8i32, list);
    add_pawn_promos(promo, -8i32, list);

    let double = ((single & BB_RANK_3) << 8) & !occ;
    add_pawn_moves(double, -16i32, list);

    let cap_left = (pawns << 9) & !BB_FILE_A & their;
    let cap_right = (pawns << 7) & !BB_FILE_H & their;
    add_pawn_moves(cap_left & !BB_RANK_8, -9i32, list);
    add_pawn_moves(cap_right & !BB_RANK_8, -7i32, list);
    add_pawn_promos(cap_left & BB_RANK_8, -9i32, list);
    add_pawn_promos(cap_right & BB_RANK_8, -7i32, list);

    if ep_sq != NO_SQUARE {
        let ep_bb = bb(ep_sq);
        if (pawns << 9) & !BB_FILE_A & ep_bb != 0 {
            list.push(make_move_ep((ep_sq as i32 - 9) as Square, ep_sq));
        }
        if (pawns << 7) & !BB_FILE_H & ep_bb != 0 {
            list.push(make_move_ep((ep_sq as i32 - 7) as Square, ep_sq));
        }
    }
}

pub(super) fn gen_pawn_moves_black(
    pawns: Bb,
    their: Bb,
    ep_sq: Square,
    occ: Bb,
    list: &mut MoveList,
) {
    let single = (pawns >> 8) & !occ;
    let promo = single & BB_RANK_1;
    let normal = single & !BB_RANK_1;
    add_pawn_moves(normal, 8i32, list);
    add_pawn_promos(promo, 8i32, list);

    let double = ((single & BB_RANK_6) >> 8) & !occ;
    add_pawn_moves(double, 16i32, list);

    let cap_left = (pawns >> 7) & !BB_FILE_A & their;
    let cap_right = (pawns >> 9) & !BB_FILE_H & their;
    add_pawn_moves(cap_left & !BB_RANK_1, 7i32, list);
    add_pawn_moves(cap_right & !BB_RANK_1, 9i32, list);
    add_pawn_promos(cap_left & BB_RANK_1, 7i32, list);
    add_pawn_promos(cap_right & BB_RANK_1, 9i32, list);

    if ep_sq != NO_SQUARE {
        let ep_bb = bb(ep_sq);
        if (pawns >> 7) & !BB_FILE_A & ep_bb != 0 {
            list.push(make_move_ep((ep_sq as i32 + 7) as Square, ep_sq));
        }
        if (pawns >> 9) & !BB_FILE_H & ep_bb != 0 {
            list.push(make_move_ep((ep_sq as i32 + 9) as Square, ep_sq));
        }
    }
}

pub(super) fn gen_pawn_captures_white(
    pawns: Bb,
    their: Bb,
    ep_sq: Square,
    occ: Bb,
    list: &mut MoveList,
) {
    let cap_left = (pawns << 9) & !BB_FILE_A & their;
    let cap_right = (pawns << 7) & !BB_FILE_H & their;
    // Push-promotions require an EMPTY target square. Without the !occ mask
    // a blocked pawn "promotes onto" the blocker — and when the blocker is
    // the enemy king, make_move captures the king and corrupts the board.
    add_pawn_promos((pawns << 8) & BB_RANK_8 & !occ, -8i32, list);
    add_pawn_promos(cap_left & BB_RANK_8, -9i32, list);
    add_pawn_promos(cap_right & BB_RANK_8, -7i32, list);
    add_pawn_moves(cap_left & !BB_RANK_8, -9i32, list);
    add_pawn_moves(cap_right & !BB_RANK_8, -7i32, list);
    if ep_sq != NO_SQUARE {
        let ep_bb = bb(ep_sq);
        if (pawns << 9) & !BB_FILE_A & ep_bb != 0 {
            list.push(make_move_ep((ep_sq as i32 - 9) as Square, ep_sq));
        }
        if (pawns << 7) & !BB_FILE_H & ep_bb != 0 {
            list.push(make_move_ep((ep_sq as i32 - 7) as Square, ep_sq));
        }
    }
}

pub(super) fn gen_pawn_captures_black(
    pawns: Bb,
    their: Bb,
    ep_sq: Square,
    occ: Bb,
    list: &mut MoveList,
) {
    let cap_left = (pawns >> 7) & !BB_FILE_A & their;
    let cap_right = (pawns >> 9) & !BB_FILE_H & their;
    add_pawn_promos((pawns >> 8) & BB_RANK_1 & !occ, 8i32, list);
    add_pawn_promos(cap_left & BB_RANK_1, 7i32, list);
    add_pawn_promos(cap_right & BB_RANK_1, 9i32, list);
    add_pawn_moves(cap_left & !BB_RANK_1, 7i32, list);
    add_pawn_moves(cap_right & !BB_RANK_1, 9i32, list);
    if ep_sq != NO_SQUARE {
        let ep_bb = bb(ep_sq);
        if (pawns >> 7) & !BB_FILE_A & ep_bb != 0 {
            list.push(make_move_ep((ep_sq as i32 + 7) as Square, ep_sq));
        }
        if (pawns >> 9) & !BB_FILE_H & ep_bb != 0 {
            list.push(make_move_ep((ep_sq as i32 + 9) as Square, ep_sq));
        }
    }
}

pub(super) fn add_pawn_moves(mut targets: Bb, from_delta: i32, list: &mut MoveList) {
    while targets != 0 {
        let to = bb_pop_lsb(&mut targets);
        let from = (to as i32 + from_delta) as Square;
        list.push(make_move(from, to));
    }
}

pub(super) fn add_pawn_promos(mut targets: Bb, from_delta: i32, list: &mut MoveList) {
    while targets != 0 {
        let to = bb_pop_lsb(&mut targets);
        let from = (to as i32 + from_delta) as Square;
        list.push(make_move_promo(from, to, PieceType::Queen));
        list.push(make_move_promo(from, to, PieceType::Rook));
        list.push(make_move_promo(from, to, PieceType::Bishop));
        list.push(make_move_promo(from, to, PieceType::Knight));
    }
}

pub(super) fn add_moves(from: Square, mut targets: Bb, list: &mut MoveList) {
    while targets != 0 {
        let to = bb_pop_lsb(&mut targets);
        list.push(make_move(from, to));
    }
}

pub(super) fn gen_castling(
    board: &Board,
    king_sq: Square,
    us: Color,
    occ: Bb,
    list: &mut MoveList,
) {
    let (ks_right, qs_right) = if us == Color::White {
        (CR_WHITE_KINGSIDE, CR_WHITE_QUEENSIDE)
    } else {
        (CR_BLACK_KINGSIDE, CR_BLACK_QUEENSIDE)
    };
    let them = us.flip();

    if board.castling & ks_right != 0 {
        let path = bb(king_sq + 1) | bb(king_sq + 2);
        if occ & path == 0
            && !board.is_attacked_by(king_sq, them)
            && !board.is_attacked_by(king_sq + 1, them)
            && !board.is_attacked_by(king_sq + 2, them)
        {
            list.push(make_move_castling(king_sq, king_sq + 2));
        }
    }

    if board.castling & qs_right != 0 {
        let path = bb(king_sq - 1) | bb(king_sq - 2) | bb(king_sq - 3);
        if occ & path == 0
            && !board.is_attacked_by(king_sq, them)
            && !board.is_attacked_by(king_sq - 1, them)
            && !board.is_attacked_by(king_sq - 2, them)
        {
            list.push(make_move_castling(king_sq, king_sq - 2));
        }
    }
}
