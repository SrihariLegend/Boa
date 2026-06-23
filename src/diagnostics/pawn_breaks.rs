use super::*;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::diagnostics) struct PawnBreakCounts {
    pub(in crate::diagnostics) total: u32,
    pub(in crate::diagnostics) liberating: u32,
}

pub(in crate::diagnostics) fn count_pawn_breaks(
    board: &Board,
    atk: &AttackTables,
    z: &Zobrist,
    color: Color,
) -> PawnBreakCounts {
    let before_mobility = mobility_for(board, atk, color);
    let mut counts = PawnBreakCounts::default();

    for m in legal_moves_for_color(board, atk, z, color) {
        if !is_pawn_break(board, color, m) {
            continue;
        }
        counts.total += 1;

        let mut next = board.clone();
        prepare_side_to_move(&mut next, color);
        let undo = next.make_move(m, z);
        let after_mobility = mobility_for(&next, atk, color);
        next.unmake_move(m, &undo, z);

        if after_mobility > before_mobility + LIBERATING_MOBILITY_GAIN {
            counts.liberating += 1;
        }
    }

    counts
}

pub(in crate::diagnostics) fn is_pawn_break(board: &Board, color: Color, m: Move) -> bool {
    let from = move_from(m);
    let to = move_to(m);
    let mover = board.sq_piece[from as usize];
    if mover == PIECE_NONE || piece_color(mover) != color || piece_type(mover) != PieceType::Pawn {
        return false;
    }

    let captures = board.sq_piece[to as usize] != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
    let opens_source_file = captures && source_file_is_clear_after_move(board, color, from);
    let creates_passer = !is_passed_pawn(board, color, from) && {
        let mut next = board.clone();
        prepare_side_to_move(&mut next, color);
        let z = Zobrist::new();
        let undo = next.make_move(m, &z);
        let passed = move_flags(m) != MF_PROMOTION && is_passed_pawn(&next, color, to);
        next.unmake_move(m, &undo, &z);
        passed
    };

    opens_source_file || creates_passer
}

pub(in crate::diagnostics) fn source_file_is_clear_after_move(
    board: &Board,
    color: Color,
    from: Square,
) -> bool {
    let pawns = board.pieces[color as usize][PieceType::Pawn as usize];
    let file = BB_FILES[sq_file(from) as usize];
    pawns & file & !bb(from) == 0
}

pub(in crate::diagnostics) fn is_passed_pawn(board: &Board, color: Color, sq: Square) -> bool {
    let file = sq_file(sq);
    let rank = sq_rank(sq);
    let mut files = BB_FILES[file as usize];
    if file > 0 {
        files |= BB_FILES[(file - 1) as usize];
    }
    if file < 7 {
        files |= BB_FILES[(file + 1) as usize];
    }

    let their_pawns = board.pieces[color.flip() as usize][PieceType::Pawn as usize];
    their_pawns & ranks_ahead(color, rank, files) == 0
}

pub(in crate::diagnostics) fn ranks_ahead(color: Color, rank: u8, files: Bb) -> Bb {
    let mut ranks = 0u64;
    if color == Color::White {
        for r in (rank + 1)..8 {
            ranks |= BB_RANKS[r as usize];
        }
    } else {
        for r in 0..rank {
            ranks |= BB_RANKS[r as usize];
        }
    }
    ranks & files
}
