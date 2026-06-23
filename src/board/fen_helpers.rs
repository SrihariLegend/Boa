use super::*;
pub(super) fn piece_to_fen_char(p: Piece) -> char {
    let c = piece_color(p);
    let ch = piece_type(p).char_lower();
    if c == Color::White {
        ch.to_ascii_uppercase()
    } else {
        ch
    }
}

/// Display character for a piece (uppercase=white, lowercase=black, '.'=empty).
pub(super) fn display_piece_char(p: Piece) -> char {
    if p == PIECE_NONE {
        return '.';
    }
    piece_to_fen_char(p)
}

/// Push castling right characters onto a FEN string.
#[allow(dead_code)]
pub(super) fn push_castling_chars(s: &mut String, castling: u8) {
    if castling & CR_WHITE_KINGSIDE != 0 {
        s.push('K');
    }
    if castling & CR_WHITE_QUEENSIDE != 0 {
        s.push('Q');
    }
    if castling & CR_BLACK_KINGSIDE != 0 {
        s.push('k');
    }
    if castling & CR_BLACK_QUEENSIDE != 0 {
        s.push('q');
    }
}
