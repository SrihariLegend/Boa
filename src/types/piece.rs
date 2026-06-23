// ---- Piece types ----

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum PieceType {
    Pawn = 0,
    Knight = 1,
    Bishop = 2,
    Rook = 3,
    Queen = 4,
    King = 5,
    None = 6,
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
            PieceType::Pawn => 100,
            PieceType::Knight => 320,
            PieceType::Bishop => 330,
            PieceType::Rook => 500,
            PieceType::Queen => 900,
            PieceType::King => 20000,
            PieceType::None => 0,
        }
    }
    pub fn char_lower(self) -> char {
        match self {
            PieceType::Pawn => 'p',
            PieceType::Knight => 'n',
            PieceType::Bishop => 'b',
            PieceType::Rook => 'r',
            PieceType::Queen => 'q',
            PieceType::King => 'k',
            PieceType::None => '.',
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
            _ => PieceType::None,
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
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
    pub fn index(self) -> usize {
        self as usize
    }
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
    if p < 6 {
        Color::White
    } else {
        Color::Black
    }
}
