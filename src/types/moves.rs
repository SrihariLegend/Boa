use super::*;
// ---- Move ----
// Packed u32:
//   bits  0-5:  from square
//   bits  6-11: to square
//   bits 12-13: promotion piece type (0=none, 1=N, 2=B, 3=R, 4=Q encoded as 0..3 → N/B/R/Q)
//   bits 14-15: move flags (0=normal, 1=en passant, 2=castling, 3=promotion)
pub type Move = u32;
pub const MOVE_NONE: Move = 0;

pub const MF_NORMAL: u32 = 0;
pub const MF_EN_PASSANT: u32 = 1;
pub const MF_CASTLING: u32 = 2;
pub const MF_PROMOTION: u32 = 3;

#[inline(always)]
pub fn move_from(m: Move) -> Square {
    (m & 0x3F) as Square
}
#[inline(always)]
pub fn move_to(m: Move) -> Square {
    ((m >> 6) & 0x3F) as Square
}
#[inline(always)]
pub fn move_flags(m: Move) -> u32 {
    (m >> 14) & 0x3
}
#[inline(always)]
pub fn move_promo_pt(m: Move) -> PieceType {
    // bits 12-13: 0=N,1=B,2=R,3=Q
    match (m >> 12) & 0x3 {
        0 => PieceType::Knight,
        1 => PieceType::Bishop,
        2 => PieceType::Rook,
        _ => PieceType::Queen,
    }
}

#[inline(always)]
pub fn make_move(from: Square, to: Square) -> Move {
    (from as u32) | ((to as u32) << 6)
}
#[inline(always)]
pub fn make_move_ep(from: Square, to: Square) -> Move {
    (from as u32) | ((to as u32) << 6) | (MF_EN_PASSANT << 14)
}
#[inline(always)]
pub fn make_move_castling(from: Square, to: Square) -> Move {
    (from as u32) | ((to as u32) << 6) | (MF_CASTLING << 14)
}
#[inline(always)]
pub fn make_move_promo(from: Square, to: Square, pt: PieceType) -> Move {
    let promo_bits: u32 = match pt {
        PieceType::Knight => 0,
        PieceType::Bishop => 1,
        PieceType::Rook => 2,
        _ => 3, // Queen
    };
    (from as u32) | ((to as u32) << 6) | (promo_bits << 12) | (MF_PROMOTION << 14)
}

pub fn move_name(m: Move) -> String {
    if m == MOVE_NONE {
        return "0000".to_string();
    }
    let mut s = format!("{}{}", sq_name(move_from(m)), sq_name(move_to(m)));
    if move_flags(m) == MF_PROMOTION {
        s.push(move_promo_pt(m).char_lower());
    }
    s
}

pub fn move_from_uci(s: &str) -> Option<Move> {
    if s.len() < 4 {
        return None;
    }
    let from = sq_from_name(&s[0..2])?;
    let to = sq_from_name(&s[2..4])?;
    if s.len() == 5 {
        let pt = PieceType::from_char(s.chars().nth(4)?);
        return Some(make_move_promo(from, to, pt));
    }
    Some(make_move(from, to))
}
