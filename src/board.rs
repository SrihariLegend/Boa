// ============================================================
// board.rs — Board representation, FEN parsing, make/unmake move
// ============================================================

#![allow(dead_code)]

use crate::types::*;

// ---- Zobrist hashing tables ----

pub struct Zobrist {
    pub piece_sq: [[[u64; 64]; 6]; 2], // [color][piece_type][square]
    pub side: u64,
    pub castling: [u64; 16],
    pub ep_file: [u64; 8],
}

impl Zobrist {
    pub fn new() -> Self {
        // Use a simple LCG to generate deterministic pseudo-random numbers
        let mut state: u64 = 0x246C_E2A1_9F27_B38F;
        let mut next = || -> u64 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut piece_sq = [[[0u64; 64]; 6]; 2];
        for c in 0..2 {
            for pt in 0..6 {
                for sq in 0..64 {
                    piece_sq[c][pt][sq] = next();
                }
            }
        }
        let side = next();
        let mut castling = [0u64; 16];
        for i in 0..16 {
            castling[i] = next();
        }
        let mut ep_file = [0u64; 8];
        for i in 0..8 {
            ep_file[i] = next();
        }
        Zobrist {
            piece_sq,
            side,
            castling,
            ep_file,
        }
    }
}

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

/// Convert a piece to its FEN character.
fn piece_to_fen_char(p: Piece) -> char {
    let c = piece_color(p);
    let ch = piece_type(p).char_lower();
    if c == Color::White {
        ch.to_ascii_uppercase()
    } else {
        ch
    }
}

/// Display character for a piece (uppercase=white, lowercase=black, '.'=empty).
fn display_piece_char(p: Piece) -> char {
    if p == PIECE_NONE {
        return '.';
    }
    piece_to_fen_char(p)
}

/// Push castling right characters onto a FEN string.
fn push_castling_chars(s: &mut String, castling: u8) {
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

impl Board {
    // ---- Construction ----

    pub fn new() -> Self {
        Board {
            pieces: [[0; 6]; 2],
            occ: [0; 2],
            occ_all: 0,
            sq_piece: [PIECE_NONE; 64],
            side: Color::White,
            castling: 0,
            ep_sq: NO_SQUARE,
            halfmove: 0,
            fullmove: 1,
            hash: 0,
            king_sq: [NO_SQUARE; 2],
        }
    }

    pub fn startpos() -> Self {
        Self::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap()
    }

    // ---- Piece helpers ----

    #[inline(always)]
    pub fn piece_bb(&self, c: Color, pt: PieceType) -> Bb {
        self.pieces[c as usize][pt as usize]
    }

    #[inline(always)]
    fn put_piece(&mut self, sq: Square, p: Piece, z: &Zobrist) {
        let c = piece_color(p) as usize;
        let pt = piece_type(p) as usize;
        self.pieces[c][pt] |= bb(sq);
        self.occ[c] |= bb(sq);
        self.occ_all |= bb(sq);
        self.sq_piece[sq as usize] = p;
        self.hash ^= z.piece_sq[c][pt][sq as usize];
        if piece_type(p) == PieceType::King {
            self.king_sq[c] = sq;
        }
    }

    #[inline(always)]
    fn remove_piece(&mut self, sq: Square, z: &Zobrist) {
        let p = self.sq_piece[sq as usize];
        if p == PIECE_NONE {
            return;
        }
        let c = piece_color(p) as usize;
        let pt = piece_type(p) as usize;
        self.pieces[c][pt] &= !bb(sq);
        self.occ[c] &= !bb(sq);
        self.occ_all &= !bb(sq);
        self.sq_piece[sq as usize] = PIECE_NONE;
        self.hash ^= z.piece_sq[c][pt][sq as usize];
    }

    // ---- FEN parsing ----

    /// Parse a single FEN piece character and place it on the board.
    fn place_fen_char(&mut self, ch: char, file: &mut i32, rank: i32, z: &Zobrist) {
        let color = if ch.is_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        let pt = PieceType::from_char(ch);
        if pt == PieceType::None || rank < 0 || rank > 7 {
            return;
        }
        let sq = sq_from(*file as u8, rank as u8);
        self.put_piece(sq, make_piece(color, pt), z);
    }

    pub fn from_fen(fen: &str) -> Option<Self> {
        let z = Zobrist::new();
        let mut board = Board::new();
        let parts: Vec<&str> = fen.split_whitespace().collect();
        if parts.len() < 4 {
            return None;
        }

        // Piece placement
        let mut rank: i32 = 7;
        let mut file: i32 = 0;
        for ch in parts[0].chars() {
            match ch {
                '/' => {
                    rank -= 1;
                    file = 0;
                }
                '1'..='8' => {
                    file += ch as i32 - '0' as i32;
                }
                c => {
                    board.place_fen_char(c, &mut file, rank, &z);
                    file += 1;
                }
            }
        }

        // Side to move
        board.side = if parts[1] == "b" {
            Color::Black
        } else {
            Color::White
        };
        if board.side == Color::Black {
            board.hash ^= z.side;
        }

        // Castling rights
        let cr_str = parts[2];
        if cr_str.contains('K') {
            board.castling |= CR_WHITE_KINGSIDE;
        }
        if cr_str.contains('Q') {
            board.castling |= CR_WHITE_QUEENSIDE;
        }
        if cr_str.contains('k') {
            board.castling |= CR_BLACK_KINGSIDE;
        }
        if cr_str.contains('q') {
            board.castling |= CR_BLACK_QUEENSIDE;
        }
        board.hash ^= z.castling[board.castling as usize];

        // En passant — same capturable-only filter as make_move so positions
        // hash identically whether reached via FEN or via moves.
        if parts[3] != "-" {
            if let Some(sq) = sq_from_name(parts[3]) {
                let (pawn_sq, capturer) = if sq_rank(sq) == 2 {
                    (sq + 8, Color::Black) // white double-pushed to rank 4
                } else {
                    (sq.wrapping_sub(8), Color::White) // black double-pushed to rank 5
                };
                let adjacent =
                    ((bb(pawn_sq) << 1) & !BB_FILE_A) | ((bb(pawn_sq) >> 1) & !BB_FILE_H);
                if board.pieces[capturer as usize][PieceType::Pawn as usize] & adjacent != 0 {
                    board.ep_sq = sq;
                    board.hash ^= z.ep_file[sq_file(sq) as usize];
                }
            }
        }

        // Halfmove and fullmove
        if parts.len() > 4 {
            board.halfmove = parts[4].parse().unwrap_or(0);
        }
        if parts.len() > 5 {
            board.fullmove = parts[5].parse().unwrap_or(1);
        }

        Some(board)
    }

    // ---- FEN export ----

    pub fn to_fen(&self) -> String {
        let mut s = String::new();
        for rank in (0..8).rev() {
            let mut empty = 0u8;
            for file in 0..8u8 {
                let sq = sq_from(file, rank);
                let p = self.sq_piece[sq as usize];
                if p == PIECE_NONE {
                    empty += 1;
                    continue;
                }
                if empty > 0 {
                    s.push((b'0' + empty) as char);
                    empty = 0;
                }
                s.push(piece_to_fen_char(p));
            }
            if empty > 0 {
                s.push((b'0' + empty) as char);
            }
            if rank > 0 {
                s.push('/');
            }
        }
        s.push(' ');
        s.push(if self.side == Color::White { 'w' } else { 'b' });
        s.push(' ');
        if self.castling == 0 {
            s.push('-');
        } else {
            push_castling_chars(&mut s, self.castling);
        }
        s.push(' ');
        if self.ep_sq == NO_SQUARE {
            s.push('-');
        } else {
            s.push_str(&sq_name(self.ep_sq));
        }
        s.push(' ');
        s.push_str(&self.halfmove.to_string());
        s.push(' ');
        s.push_str(&self.fullmove.to_string());
        s
    }

    // ---- Make/Unmake move ----

    pub fn make_move(&mut self, m: Move, z: &Zobrist) -> UndoInfo {
        let undo = UndoInfo {
            captured: PIECE_NONE,
            ep_sq: self.ep_sq,
            castling: self.castling,
            halfmove: self.halfmove,
            hash: self.hash,
        };
        let from = move_from(m);
        let to = move_to(m);
        let flags = move_flags(m);
        let mover = self.sq_piece[from as usize];
        let mover_type = piece_type(mover);
        let us = self.side;
        let them = us.flip();

        // Clear old ep hash
        if self.ep_sq != NO_SQUARE {
            self.hash ^= z.ep_file[sq_file(self.ep_sq) as usize];
        }

        // Clear castling hash
        self.hash ^= z.castling[self.castling as usize];

        // Capture
        let mut undo = undo;
        if flags == MF_EN_PASSANT {
            let cap_sq = if us == Color::White { to - 8 } else { to + 8 };
            undo.captured = self.sq_piece[cap_sq as usize];
            self.remove_piece(cap_sq, z);
        } else {
            let captured = self.sq_piece[to as usize];
            if captured != PIECE_NONE {
                undo.captured = captured;
                self.remove_piece(to, z);
            }
        }

        // Move piece
        self.remove_piece(from, z);
        if flags == MF_PROMOTION {
            let promo = make_piece(us, move_promo_pt(m));
            self.put_piece(to, promo, z);
        } else {
            self.put_piece(to, mover, z);
        }

        // Castling rook
        if flags == MF_CASTLING {
            let (rook_from, rook_to) = if to > from {
                // Kingside
                (from + 3, from + 1)
            } else {
                // Queenside
                (from - 4, from - 1)
            };
            let rook = make_piece(us, PieceType::Rook);
            self.remove_piece(rook_from, z);
            self.put_piece(rook_to, rook, z);
        }

        // Update castling rights
        self.castling &= CASTLING_RIGHTS_MASK[from as usize] & CASTLING_RIGHTS_MASK[to as usize];
        self.hash ^= z.castling[self.castling as usize];

        // En passant square — only recorded when an enemy pawn can actually
        // capture. Otherwise the same position reached via double-push vs
        // single-steps would hash differently and break repetition detection.
        self.ep_sq = NO_SQUARE;
        if mover_type == PieceType::Pawn {
            let diff = (to as i32 - from as i32).abs();
            if diff == 16 {
                let adjacent = ((bb(to) << 1) & !BB_FILE_A) | ((bb(to) >> 1) & !BB_FILE_H);
                if self.pieces[them as usize][PieceType::Pawn as usize] & adjacent != 0 {
                    self.ep_sq = (from + to) / 2;
                    self.hash ^= z.ep_file[sq_file(self.ep_sq) as usize];
                }
            }
        }

        // Halfmove clock
        if mover_type == PieceType::Pawn || undo.captured != PIECE_NONE {
            self.halfmove = 0;
        } else {
            self.halfmove += 1;
        }

        // Full move
        if us == Color::Black {
            self.fullmove += 1;
        }

        // Side to move
        self.side = them;
        self.hash ^= z.side;

        undo
    }

    pub fn unmake_move(&mut self, m: Move, undo: &UndoInfo, z: &Zobrist) {
        let from = move_from(m);
        let to = move_to(m);
        let flags = move_flags(m);
        let them = self.side; // was opponent during the move
        let us = them.flip();

        // Restore side
        self.side = us;

        // Move piece back
        let moved_piece = if flags == MF_PROMOTION {
            make_piece(us, PieceType::Pawn)
        } else {
            self.sq_piece[to as usize]
        };
        self.remove_piece(to, z);
        self.put_piece(from, moved_piece, z);

        // Restore capture
        if flags == MF_EN_PASSANT {
            let cap_sq = if us == Color::White { to - 8 } else { to + 8 };
            self.put_piece(cap_sq, undo.captured, z);
        } else if undo.captured != PIECE_NONE {
            self.put_piece(to, undo.captured, z);
        }

        // Restore castling rook
        if flags == MF_CASTLING {
            let (rook_from, rook_to) = if to > from {
                (from + 3, from + 1)
            } else {
                (from - 4, from - 1)
            };
            let rook = make_piece(us, PieceType::Rook);
            self.remove_piece(rook_to, z);
            self.put_piece(rook_from, rook, z);
        }

        // Restore state
        self.ep_sq = undo.ep_sq;
        self.castling = undo.castling;
        self.halfmove = undo.halfmove;
        self.hash = undo.hash;

        if us == Color::Black {
            self.fullmove -= 1;
        }
    }

    // ---- Null move: just flips side, clears EP, updates hash ----

    pub fn make_null_move(&mut self, z: &Zobrist) -> UndoInfo {
        let undo = UndoInfo {
            captured: PIECE_NONE,
            ep_sq: self.ep_sq,
            castling: self.castling,
            halfmove: self.halfmove,
            hash: self.hash,
        };

        // Clear old ep hash
        if self.ep_sq != NO_SQUARE {
            self.hash ^= z.ep_file[sq_file(self.ep_sq) as usize];
            self.ep_sq = NO_SQUARE;
        }

        // Flip side
        self.side = self.side.flip();
        self.hash ^= z.side;

        // Increment halfmove
        self.halfmove += 1;

        undo
    }

    pub fn unmake_null_move(&mut self, undo: &UndoInfo) {
        self.side = self.side.flip();
        self.ep_sq = undo.ep_sq;
        self.halfmove = undo.halfmove;
        self.hash = undo.hash;
    }

    // ---- Check detection ----

    pub fn is_in_check(&self, color: Color) -> bool {
        let king_sq = self.king_sq[color as usize];
        if king_sq == NO_SQUARE {
            return false;
        }
        self.is_attacked_by(king_sq, color.flip())
    }

    pub fn is_attacked_by(&self, sq: Square, attacker_color: Color) -> bool {
        use crate::movegen::*;
        let ac = attacker_color as usize;
        let occ = self.occ_all;

        // Pawn attacks
        let pawn_attacks = if attacker_color == Color::White {
            pawn_attacks_white(self.pieces[ac][PieceType::Pawn as usize])
        } else {
            pawn_attacks_black(self.pieces[ac][PieceType::Pawn as usize])
        };
        if pawn_attacks & bb(sq) != 0 {
            return true;
        }

        // Knight attacks
        if knight_attacks(sq) & self.pieces[ac][PieceType::Knight as usize] != 0 {
            return true;
        }

        // Bishop/Queen diagonal
        let diag_attackers = self.pieces[ac][PieceType::Bishop as usize]
            | self.pieces[ac][PieceType::Queen as usize];
        if bishop_attacks(sq, occ) & diag_attackers != 0 {
            return true;
        }

        // Rook/Queen straight
        let straight_attackers =
            self.pieces[ac][PieceType::Rook as usize] | self.pieces[ac][PieceType::Queen as usize];
        if rook_attacks(sq, occ) & straight_attackers != 0 {
            return true;
        }

        // King attacks
        if king_attacks(sq) & self.pieces[ac][PieceType::King as usize] != 0 {
            return true;
        }

        false
    }

    // ---- Display ----

    pub fn display(&self) {
        println!("+--------+");
        for rank in (0..8).rev() {
            print!("|");
            for file in 0..8u8 {
                let sq = sq_from(file, rank);
                let p = self.sq_piece[sq as usize];
                print!("{}", display_piece_char(p));
            }
            println!("|{}", rank + 1);
        }
        println!("+--------+");
        println!(" abcdefgh");
        println!("Side: {:?}, Hash: {:016x}", self.side, self.hash);
    }

    // ---- Non-pawn material (for game phase) ----
    pub fn non_pawn_material(&self, c: Color) -> i32 {
        let ci = c as usize;
        let n = bb_popcount(self.pieces[ci][PieceType::Knight as usize]) as i32;
        let b = bb_popcount(self.pieces[ci][PieceType::Bishop as usize]) as i32;
        let r = bb_popcount(self.pieces[ci][PieceType::Rook as usize]) as i32;
        let q = bb_popcount(self.pieces[ci][PieceType::Queen as usize]) as i32;
        n * 320 + b * 330 + r * 500 + q * 900
    }
}

// Castling rights mask: after a move from/to these squares, which castling rights survive?
// Values are 4-bit masks ANDed with current castling rights.
// Using explicit values instead of !CR_* to avoid u8 bitwise complement (gives 253 not 13).
#[rustfmt::skip]
const CASTLING_RIGHTS_MASK: [u8; 64] = [
    // A1(0)           B1  C1  D1   E1(4)           F1  G1   H1(7)
    13,               15, 15, 15,  12,              15, 15,  14,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
    // A8(56)          B8  C8  D8   E8(60)          F8  G8   H8(63)
     7,               15, 15, 15,   3,              15, 15,  11,
];
