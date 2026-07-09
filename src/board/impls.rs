use super::*;
impl Board {
    // ---- Construction ----

    /// Returns a blank board with `hash == 0`. The hash is NOT valid for
    /// searching — it lacks the Zobrist castling component (`z.castling[0]`).
    /// This constructor is only meant as a base for [`from_fen`], which
    /// corrects the hash. Never call `make_move` on a board created directly
    /// via `new()` without first going through `from_fen`.
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
            pawn_hash: 0,
            king_sq: [NO_SQUARE; 2],
        }
    }

    pub fn startpos() -> Self {
        Self::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap()
    }

    // ---- Piece helpers ----

    #[inline(always)]
    #[allow(dead_code)]
    pub fn piece_bb(&self, c: Color, pt: PieceType) -> Bb {
        self.pieces[c as usize][pt as usize]
    }

    #[inline(always)]
    fn put_piece(&mut self, sq: Square, p: Piece, z: &Zobrist) {
        debug_assert!(
            p != PIECE_NONE,
            "put_piece called with PIECE_NONE — would corrupt bitboards"
        );
        if p == PIECE_NONE {
            return;
        }
        let c = piece_color(p) as usize;
        let pt = piece_type(p) as usize;
        self.pieces[c][pt] |= bb(sq);
        self.occ[c] |= bb(sq);
        self.occ_all |= bb(sq);
        self.sq_piece[sq as usize] = p;
        self.hash ^= z.piece_sq[c][pt][sq as usize];
        if pt == PieceType::Pawn as usize {
            self.pawn_hash ^= z.piece_sq[c][pt][sq as usize];
        }
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
        // Clear king_sq if the removed piece was a king.
        // This prevents stale king_sq from causing gen_moves to generate
        // moves from an empty square, which would cascade into put_piece(PIECE_NONE)
        // board corruption.
        if pt == PieceType::King as usize {
            self.king_sq[c] = NO_SQUARE;
        }
        self.hash ^= z.piece_sq[c][pt][sq as usize];
        if pt == PieceType::Pawn as usize {
            self.pawn_hash ^= z.piece_sq[c][pt][sq as usize];
        }
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
        if pt == PieceType::None || !(0..=7).contains(&rank) {
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
            // Malformed FEN with >8 files on a rank would push `file`
            // past 7 inside place_fen_char → sq_from panics.
            if file > 8 {
                return None;
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

    #[allow(dead_code)]
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

    /// Verify that sq_piece and the bitboard arrays are consistent.
    /// Returns the first inconsistent square found, if any.
    pub fn verify_consistency(&self) -> Option<String> {
        for sq in 0..64u8 {
            let p = self.sq_piece[sq as usize];
            if p == PIECE_NONE {
                // sq_piece says empty — check that no bitboard claims this square
                for c in 0..2usize {
                    for pt in 0..6usize {
                        if self.pieces[c][pt] & bb(sq) != 0 {
                            return Some(format!(
                                "sq={} sq_piece=NONE but pieces[{}][{}] has bit set. fen={}",
                                sq,
                                c,
                                pt,
                                self.to_fen()
                            ));
                        }
                    }
                }
            } else {
                let c = piece_color(p) as usize;
                let pt = piece_type(p) as usize;
                if self.pieces[c][pt] & bb(sq) == 0 {
                    return Some(format!(
                        "sq={} sq_piece={:?} but pieces[{}][{}] bit NOT set. fen={}",
                        sq,
                        p,
                        c,
                        pt,
                        self.to_fen()
                    ));
                }
                // Check occupancy
                if self.occ[c] & bb(sq) == 0 {
                    return Some(format!(
                        "sq={} sq_piece={:?} but occ[{}] bit NOT set. fen={}",
                        sq,
                        p,
                        c,
                        self.to_fen()
                    ));
                }
                if self.occ_all & bb(sq) == 0 {
                    return Some(format!(
                        "sq={} sq_piece={:?} but occ_all bit NOT set. fen={}",
                        sq,
                        p,
                        self.to_fen()
                    ));
                }
            }
            // Check that occupied squares in bitboards have matching sq_piece
            for c in 0..2usize {
                if self.occ[c] & bb(sq) != 0 {
                    let p_at = self.sq_piece[sq as usize];
                    if p_at == PIECE_NONE {
                        return Some(format!(
                            "sq={} occ[{}] has bit but sq_piece=NONE. fen={}",
                            sq,
                            c,
                            self.to_fen()
                        ));
                    }
                    if piece_color(p_at) as usize != c {
                        return Some(format!(
                            "sq={} occ[{}] has bit but sq_piece={:?} has wrong color. fen={}",
                            sq,
                            c,
                            p_at,
                            self.to_fen()
                        ));
                    }
                }
            }
        }
        None
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movegen::gen_moves;

    /// Helper: find the legal move matching a UCI string. Unlike move_from_uci,
    /// this resolves special move flags (castling, en passant) by searching
    /// the generated move list.
    fn find_legal_move_for_uci(
        board: &Board,
        atk: &crate::movegen::AttackTables,
        z: &Zobrist,
        uci: &str,
    ) -> Option<Move> {
        let m = move_from_uci(uci)?;
        let from = move_from(m);
        let to = move_to(m);
        let promo_flag = move_flags(m) == MF_PROMOTION;
        let list = gen_moves(board, atk);
        for &lm in list.iter() {
            if move_from(lm) != from || move_to(lm) != to {
                continue;
            }
            if promo_flag
                && (move_flags(lm) != MF_PROMOTION || move_promo_pt(lm) != move_promo_pt(m))
            {
                continue;
            }
            let mut b = board.clone();
            let _undo = b.make_move(lm, z);
            if !b.is_in_check(b.side.flip()) {
                return Some(lm);
            }
        }
        None
    }

    #[test]
    fn board_consistency_after_problem_position() {
        let mut board =
            Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        assert_eq!(
            board.verify_consistency(),
            None,
            "inconsistent after startpos"
        );

        let z = Zobrist::new();
        let atk = crate::movegen::AttackTables::init();
        let moves_str =
            "e2e4 e7e5 g1f3 b8c6 f1b5 g8f6 e1g1 f6e4 d2d4 e5d4 f1e1 f7f5 f3d4 c6d4 d1d4 f8e7 d4g7";
        for m_str in moves_str.split_whitespace() {
            let lm = find_legal_move_for_uci(&board, &atk, &z, m_str)
                .unwrap_or_else(|| panic!("no legal move for {} in {}", m_str, board.to_fen()));
            board.make_move(lm, &z);
            if let Some(err) = board.verify_consistency() {
                panic!("inconsistent after move {}: {}", m_str, err);
            }
        }

        // After Qxg7, neither side is in check
        assert!(
            !board.is_in_check(Color::White),
            "White should NOT be in check: {}",
            board.to_fen()
        );
        assert!(
            !board.is_in_check(Color::Black),
            "Black should NOT be in check: {}",
            board.to_fen()
        );
    }
}

// Castling rights mask: after a move from/to these squares, which castling rights survive?
// Values are 4-bit masks ANDed with current castling rights.
// Using explicit values instead of !CR_* to avoid u8 bitwise complement (gives 253 not 13).
