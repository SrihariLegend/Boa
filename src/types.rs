// ============================================================
// types.rs — Core types, constants, and primitives
// ============================================================

#![allow(dead_code)]

// ---- Squares ----

pub type Square = u8;

pub const A1: Square = 0;  pub const B1: Square = 1;  pub const C1: Square = 2;  pub const D1: Square = 3;
pub const E1: Square = 4;  pub const F1: Square = 5;  pub const G1: Square = 6;  pub const H1: Square = 7;
pub const A2: Square = 8;  pub const B2: Square = 9;  pub const C2: Square = 10; pub const D2: Square = 11;
pub const E2: Square = 12; pub const F2: Square = 13; pub const G2: Square = 14; pub const H2: Square = 15;
pub const A3: Square = 16; pub const B3: Square = 17; pub const C3: Square = 18; pub const D3: Square = 19;
pub const E3: Square = 20; pub const F3: Square = 21; pub const G3: Square = 22; pub const H3: Square = 23;
pub const A4: Square = 24; pub const B4: Square = 25; pub const C4: Square = 26; pub const D4: Square = 27;
pub const E4: Square = 28; pub const F4: Square = 29; pub const G4: Square = 30; pub const H4: Square = 31;
pub const A5: Square = 32; pub const B5: Square = 33; pub const C5: Square = 34; pub const D5: Square = 35;
pub const E5: Square = 36; pub const F5: Square = 37; pub const G5: Square = 38; pub const H5: Square = 39;
pub const A6: Square = 40; pub const B6: Square = 41; pub const C6: Square = 42; pub const D6: Square = 43;
pub const E6: Square = 44; pub const F6: Square = 45; pub const G6: Square = 46; pub const H6: Square = 47;
pub const A7: Square = 48; pub const B7: Square = 49; pub const C7: Square = 50; pub const D7: Square = 51;
pub const E7: Square = 52; pub const F7: Square = 53; pub const G7: Square = 54; pub const H7: Square = 55;
pub const A8: Square = 56; pub const B8: Square = 57; pub const C8: Square = 58; pub const D8: Square = 59;
pub const E8: Square = 60; pub const F8: Square = 61; pub const G8: Square = 62; pub const H8: Square = 63;

pub const NO_SQUARE: Square = 64;

pub fn sq_file(sq: Square) -> u8 { sq & 7 }
pub fn sq_rank(sq: Square) -> u8 { sq >> 3 }
pub fn sq_from(file: u8, rank: u8) -> Square { rank * 8 + file }
pub fn sq_name(sq: Square) -> String {
    let file = b'a' + sq_file(sq);
    let rank = b'1' + sq_rank(sq);
    format!("{}{}", file as char, rank as char)
}
pub fn sq_from_name(s: &str) -> Option<Square> {
    let mut chars = s.chars();
    let file = chars.next()? as u8;
    let rank = chars.next()? as u8;
    if file < b'a' || file > b'h' || rank < b'1' || rank > b'8' { return None; }
    Some(sq_from(file - b'a', rank - b'1'))
}

// ---- Bitboard ----

pub type Bb = u64;

pub const BB_EMPTY: Bb = 0;
pub const BB_ALL: Bb = !0;
pub const BB_RANK_1: Bb = 0x00000000000000FF;
pub const BB_RANK_2: Bb = 0x000000000000FF00;
pub const BB_RANK_3: Bb = 0x0000000000FF0000;
pub const BB_RANK_4: Bb = 0x00000000FF000000;
pub const BB_RANK_5: Bb = 0x000000FF00000000;
pub const BB_RANK_6: Bb = 0x0000FF0000000000;
pub const BB_RANK_7: Bb = 0x00FF000000000000;
pub const BB_RANK_8: Bb = 0xFF00000000000000;
pub const BB_FILE_A: Bb = 0x0101010101010101;
pub const BB_FILE_B: Bb = 0x0202020202020202;
pub const BB_FILE_C: Bb = 0x0404040404040404;
pub const BB_FILE_D: Bb = 0x0808080808080808;
pub const BB_FILE_E: Bb = 0x1010101010101010;
pub const BB_FILE_F: Bb = 0x2020202020202020;
pub const BB_FILE_G: Bb = 0x4040404040404040;
pub const BB_FILE_H: Bb = 0x8080808080808080;
pub const BB_FILES: [Bb; 8] = [
    BB_FILE_A, BB_FILE_B, BB_FILE_C, BB_FILE_D,
    BB_FILE_E, BB_FILE_F, BB_FILE_G, BB_FILE_H,
];
pub const BB_RANKS: [Bb; 8] = [
    BB_RANK_1, BB_RANK_2, BB_RANK_3, BB_RANK_4,
    BB_RANK_5, BB_RANK_6, BB_RANK_7, BB_RANK_8,
];
pub const BB_LIGHT_SQUARES: Bb = 0x55AA55AA55AA55AA;
pub const BB_DARK_SQUARES:  Bb = 0xAA55AA55AA55AA55;
pub const BB_CENTER:        Bb = (1u64 << D4) | (1u64 << E4) | (1u64 << D5) | (1u64 << E5);
pub const BB_EXTENDED_CENTER: Bb =
    (1u64 << C3) | (1u64 << D3) | (1u64 << E3) | (1u64 << F3) |
    (1u64 << C4) | (1u64 << D4) | (1u64 << E4) | (1u64 << F4) |
    (1u64 << C5) | (1u64 << D5) | (1u64 << E5) | (1u64 << F5) |
    (1u64 << C6) | (1u64 << D6) | (1u64 << E6) | (1u64 << F6);

#[inline(always)]
pub fn bb(sq: Square) -> Bb { 1u64 << sq }

#[inline(always)]
pub fn bb_lsb(b: Bb) -> Square { b.trailing_zeros() as Square }

#[inline(always)]
pub fn bb_pop_lsb(b: &mut Bb) -> Square {
    let sq = bb_lsb(*b);
    *b &= *b - 1;
    sq
}

#[inline(always)]
pub fn bb_popcount(b: Bb) -> u32 { b.count_ones() }

// ---- Piece types ----

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum PieceType {
    Pawn   = 0,
    Knight = 1,
    Bishop = 2,
    Rook   = 3,
    Queen  = 4,
    King   = 5,
    None   = 6,
}

impl PieceType {
    pub fn from_u8(v: u8) -> PieceType {
        match v {
            0 => PieceType::Pawn,
            1 => PieceType::Knight,
            2 => PieceType::Bishop,
            3 => PieceType::Rook,
            4 => PieceType::Queen,
            5 => PieceType::King,
            _ => PieceType::None,
        }
    }
    pub fn material_value(self) -> i32 {
        match self {
            PieceType::Pawn   => 100,
            PieceType::Knight => 320,
            PieceType::Bishop => 330,
            PieceType::Rook   => 500,
            PieceType::Queen  => 900,
            PieceType::King   => 20000,
            PieceType::None   => 0,
        }
    }
    pub fn char_lower(self) -> char {
        match self {
            PieceType::Pawn   => 'p',
            PieceType::Knight => 'n',
            PieceType::Bishop => 'b',
            PieceType::Rook   => 'r',
            PieceType::Queen  => 'q',
            PieceType::King   => 'k',
            PieceType::None   => '.',
        }
    }
    pub fn from_char(c: char) -> PieceType {
        match c.to_ascii_lowercase() {
            'p' => PieceType::Pawn,
            'n' => PieceType::Knight,
            'b' => PieceType::Bishop,
            'r' => PieceType::Rook,
            'q' => PieceType::Queen,
            'k' => PieceType::King,
            _   => PieceType::None,
        }
    }
}

// ---- Color ----

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    #[inline(always)]
    pub fn flip(self) -> Color {
        match self { Color::White => Color::Black, Color::Black => Color::White }
    }
    pub fn index(self) -> usize { self as usize }
}

// ---- Piece (color + type packed) ----
// Encoding: bits [0..2] = PieceType, bit 3 = Color (0=White, 1=Black), value 12 = None
pub type Piece = u8;
pub const PIECE_NONE: Piece = 12;

#[inline(always)]
pub fn make_piece(color: Color, pt: PieceType) -> Piece {
    (color as u8) * 6 + (pt as u8)
}
#[inline(always)]
pub fn piece_type(p: Piece) -> PieceType {
    PieceType::from_u8(p % 6)
}
#[inline(always)]
pub fn piece_color(p: Piece) -> Color {
    if p < 6 { Color::White } else { Color::Black }
}

// ---- Move ----
// Packed u32:
//   bits  0-5:  from square
//   bits  6-11: to square
//   bits 12-13: promotion piece type (0=none, 1=N, 2=B, 3=R, 4=Q encoded as 0..3 → N/B/R/Q)
//   bits 14-15: move flags (0=normal, 1=en passant, 2=castling, 3=promotion)
pub type Move = u32;
pub const MOVE_NONE: Move = 0;

pub const MF_NORMAL:    u32 = 0;
pub const MF_EN_PASSANT: u32 = 1;
pub const MF_CASTLING:  u32 = 2;
pub const MF_PROMOTION: u32 = 3;

#[inline(always)]
pub fn move_from(m: Move) -> Square    { (m & 0x3F) as Square }
#[inline(always)]
pub fn move_to(m: Move) -> Square      { ((m >> 6) & 0x3F) as Square }
#[inline(always)]
pub fn move_flags(m: Move) -> u32      { (m >> 14) & 0x3 }
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
        PieceType::Rook   => 2,
        _                 => 3, // Queen
    };
    (from as u32) | ((to as u32) << 6) | (promo_bits << 12) | (MF_PROMOTION << 14)
}

pub fn move_name(m: Move) -> String {
    if m == MOVE_NONE { return "0000".to_string(); }
    let mut s = format!("{}{}", sq_name(move_from(m)), sq_name(move_to(m)));
    if move_flags(m) == MF_PROMOTION {
        s.push(move_promo_pt(m).char_lower());
    }
    s
}

pub fn move_from_uci(s: &str) -> Option<Move> {
    if s.len() < 4 { return None; }
    let from = sq_from_name(&s[0..2])?;
    let to   = sq_from_name(&s[2..4])?;
    if s.len() == 5 {
        let pt = PieceType::from_char(s.chars().nth(4)?);
        return Some(make_move_promo(from, to, pt));
    }
    Some(make_move(from, to))
}

// ---- Score / evaluation ----

pub type Score = i32;
pub const SCORE_INF:     Score = 1_000_000;
pub const SCORE_MATE:    Score = 900_000;
pub const SCORE_DRAW:    Score = 0;

/// Returns true if the score is a mate score
/// Max search depth (plies). Mate scores encode ply, so this bounds the range.
pub const MAX_PLY: usize = 128;

pub fn is_mate_score(s: Score) -> bool { s.abs() >= SCORE_MATE - MAX_PLY as Score }

/// Converts a mate score to mate-in-N (positive = we mate, negative = they mate)
pub fn mate_in(s: Score) -> i32 {
    if s > 0 {
        (SCORE_MATE - s + 1) / 2
    } else {
        -(SCORE_MATE + s + 1) / 2
    }
}

// ---- Castling rights bitmask ----
pub const CR_WHITE_KINGSIDE:  u8 = 1;
pub const CR_WHITE_QUEENSIDE: u8 = 2;
pub const CR_BLACK_KINGSIDE:  u8 = 4;
pub const CR_BLACK_QUEENSIDE: u8 = 8;
pub const CR_WHITE: u8 = CR_WHITE_KINGSIDE | CR_WHITE_QUEENSIDE;
pub const CR_BLACK: u8 = CR_BLACK_KINGSIDE | CR_BLACK_QUEENSIDE;

// ---- Game phase ----
// Linear interpolation between midgame and endgame evaluation

/// Total non-pawn material when all pieces are on the board:
/// 2 × (Knight=320 + Bishop=330 + 2×Rook=1000 + Queen=900) = 5100
/// We use this as the denominator for phase interpolation.
const TOTAL_NON_PAWN_MATERIAL: i32 = 2 * (320 + 330 + 2 * 500 + 900);

pub fn game_phase(non_pawn_material: i32) -> i32 {
    let phase = (non_pawn_material * 256 / TOTAL_NON_PAWN_MATERIAL).min(256);
    phase
}
