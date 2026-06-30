use super::*;
// ---- Board state ----

#[derive(Clone)]
pub struct Board {
    // Piece bitboards: [color][piece_type]
    pub pieces: [[Bb; 6]; 2],
    // Occupancy by color and combined
    pub occ: [Bb; 2], // [color]
    pub occ_all: Bb,
    // Per-square piece lookup
    pub sq_piece: [Piece; 64],
    // Side to move
    pub side: Color,
    // Castling rights
    pub castling: u8,
    // En passant target square (NO_SQUARE if none)
    pub ep_sq: Square,
    // Fifty-move rule counter
    pub halfmove: u8,
    // Full move number (starts at 1)
    pub fullmove: u16,
    // Zobrist hash of current position
    pub hash: u64,
    /// Zobrist hash of pawn structure only (pawns of both colors).
    /// Used by pawn history table for position-type-aware move ordering.
    pub pawn_hash: u64,
    // King squares cache
    pub king_sq: [Square; 2],
}

// ---- Undo state (saved before make_move, restored by unmake_move) ----

#[derive(Clone, Copy)]
pub struct UndoInfo {
    pub captured: Piece,
    pub ep_sq: Square,
    pub castling: u8,
    pub halfmove: u8,
    pub hash: u64,
}
