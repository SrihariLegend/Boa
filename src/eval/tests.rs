use super::*;

pub(in crate::eval) fn passer_bonus_for(fen: &str, color: Color, sq: Square) -> (i32, i32) {
    let board = Board::from_fen(fen).unwrap();
    let ci = color as usize;
    let rank = sq_rank(sq);
    let file = sq_file(sq);
    let file_bb = BB_FILES[file as usize];
    let adj_files = (if file > 0 {
        BB_FILES[(file - 1) as usize]
    } else {
        0
    }) | (if file < 7 {
        BB_FILES[(file + 1) as usize]
    } else {
        0
    });
    let promo_dist = if color == Color::White {
        7 - rank
    } else {
        rank
    };

    let passed = PassedPawnContext {
        sq,
        rank,
        file,
        file_bb,
        adj_files,
        our_pawns: board.pieces[ci][PieceType::Pawn as usize],
        their_pawns: board.pieces[color.flip() as usize][PieceType::Pawn as usize],
        promo_dist,
    };

    passed_pawn_bonuses(&board, color, passed)
}

#[test]
pub(in crate::eval) fn rook_behind_white_passer_is_rewarded() {
    let without_rook = passer_bonus_for("8/8/8/4P3/8/8/8/8 w - - 0 1", Color::White, E5);
    let with_rook = passer_bonus_for("8/8/8/4P3/8/8/8/4R3 w - - 0 1", Color::White, E5);

    assert_eq!(
        (with_rook.0 - without_rook.0, with_rook.1 - without_rook.1),
        ROOK_BEHIND_PASSER_BONUS,
    );
}

#[test]
pub(in crate::eval) fn rook_in_front_of_white_passer_is_not_rewarded_as_behind() {
    let bishop_in_front = passer_bonus_for("8/8/4B3/4P3/8/8/8/8 w - - 0 1", Color::White, E5);
    let rook_in_front = passer_bonus_for("8/8/4R3/4P3/8/8/8/8 w - - 0 1", Color::White, E5);

    assert_eq!(rook_in_front, bishop_in_front);
}

#[test]
pub(in crate::eval) fn rook_behind_black_passer_is_rewarded() {
    let without_rook = passer_bonus_for("8/8/8/4p3/8/8/8/8 b - - 0 1", Color::Black, E5);
    let with_rook = passer_bonus_for("4r3/8/8/4p3/8/8/8/8 b - - 0 1", Color::Black, E5);

    assert_eq!(
        (with_rook.0 - without_rook.0, with_rook.1 - without_rook.1),
        (-ROOK_BEHIND_PASSER_BONUS.0, -ROOK_BEHIND_PASSER_BONUS.1),
    );
}

#[test]
pub(in crate::eval) fn rook_in_front_of_black_passer_is_not_rewarded_as_behind() {
    let bishop_in_front = passer_bonus_for("8/8/8/4p3/4b3/8/8/8 b - - 0 1", Color::Black, E5);
    let rook_in_front = passer_bonus_for("8/8/8/4p3/4r3/8/8/8 b - - 0 1", Color::Black, E5);

    assert_eq!(rook_in_front, bishop_in_front);
}
