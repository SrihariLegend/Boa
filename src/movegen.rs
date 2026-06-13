// ============================================================
// movegen.rs — Bitboard move generation with magic bitboards
// ============================================================
//
// Uses magic bitboards for bishop and rook sliding attacks.
// Magic numbers are found at runtime via brute-force search during init().

#![allow(dead_code)]

use crate::types::*;
use crate::board::Board;

// ============================================================
// Section 1: Attack tables
// ============================================================

pub struct AttackTables {
    pub knight:         [Bb; 64],
    pub king:           [Bb; 64],
    rook_magics:        [MagicEntry; 64],
    bishop_magics:      [MagicEntry; 64],
    rook_table:         Vec<Bb>,
    bishop_table:       Vec<Bb>,
}

#[derive(Clone, Copy)]
struct MagicEntry {
    mask:   Bb,
    magic:  u64,
    shift:  u32,
    offset: usize,
}

impl Default for MagicEntry {
    fn default() -> Self {
        MagicEntry { mask: 0, magic: 0, shift: 0, offset: 0 }
    }
}

// ---- Mask generation ----

fn gen_rook_mask(sq: Square) -> Bb {
    let r = sq_rank(sq) as i32;
    let f = sq_file(sq) as i32;
    let mut mask = 0u64;
    for i in (f + 1)..7 { mask |= 1u64 << (r * 8 + i); }
    for i in 1..f       { mask |= 1u64 << (r * 8 + i); }
    for i in (r + 1)..7 { mask |= 1u64 << (i * 8 + f); }
    for i in 1..r       { mask |= 1u64 << (i * 8 + f); }
    mask
}

fn gen_bishop_mask(sq: Square) -> Bb {
    let r = sq_rank(sq) as i32;
    let f = sq_file(sq) as i32;
    let mut mask = 0u64;
    for (dr, df) in [(1i32,1i32),(1,-1),(-1,1),(-1,-1)] {
        let mut cr = r + dr; let mut cf = f + df;
        while cr > 0 && cr < 7 && cf > 0 && cf < 7 {
            mask |= 1u64 << (cr * 8 + cf);
            cr += dr; cf += df;
        }
    }
    mask
}

// ---- Classical sliding attacks (used for table generation and fallback) ----

fn sliding_attacks_rook(sq: Square, occ: Bb) -> Bb {
    let r = sq_rank(sq) as i32;
    let f = sq_file(sq) as i32;
    let mut atk = 0u64;
    for (dr, df) in [(0i32,1i32),(0,-1),(1,0),(-1,0)] {
        let mut cr = r + dr; let mut cf = f + df;
        while cr >= 0 && cr <= 7 && cf >= 0 && cf <= 7 {
            let s = (cr * 8 + cf) as u32;
            atk |= 1u64 << s;
            if occ & (1u64 << s) != 0 { break; }
            cr += dr; cf += df;
        }
    }
    atk
}

fn sliding_attacks_bishop(sq: Square, occ: Bb) -> Bb {
    let r = sq_rank(sq) as i32;
    let f = sq_file(sq) as i32;
    let mut atk = 0u64;
    for (dr, df) in [(1i32,1i32),(1,-1),(-1,1),(-1,-1)] {
        let mut cr = r + dr; let mut cf = f + df;
        while cr >= 0 && cr <= 7 && cf >= 0 && cf <= 7 {
            let s = (cr * 8 + cf) as u32;
            atk |= 1u64 << s;
            if occ & (1u64 << s) != 0 { break; }
            cr += dr; cf += df;
        }
    }
    atk
}

// ---- Non-slider attack generation ----

fn gen_knight_attacks(sq: Square) -> Bb {
    let bb_sq = bb(sq);
    let mut atk = 0u64;
    atk |= (bb_sq << 17) & !BB_FILE_A;
    atk |= (bb_sq << 15) & !BB_FILE_H;
    atk |= (bb_sq << 10) & !(BB_FILE_A | BB_FILE_B);
    atk |= (bb_sq <<  6) & !(BB_FILE_G | BB_FILE_H);
    atk |= (bb_sq >> 17) & !BB_FILE_H;
    atk |= (bb_sq >> 15) & !BB_FILE_A;
    atk |= (bb_sq >> 10) & !(BB_FILE_G | BB_FILE_H);
    atk |= (bb_sq >>  6) & !(BB_FILE_A | BB_FILE_B);
    atk
}

fn gen_king_attacks(sq: Square) -> Bb {
    let bb_sq = bb(sq);
    let mut atk = 0u64;
    atk |= bb_sq << 8;
    atk |= bb_sq >> 8;
    atk |= (bb_sq << 1) & !BB_FILE_A;
    atk |= (bb_sq >> 1) & !BB_FILE_H;
    atk |= (bb_sq << 9) & !BB_FILE_A;
    atk |= (bb_sq >> 9) & !BB_FILE_H;
    atk |= (bb_sq << 7) & !BB_FILE_H;
    atk |= (bb_sq >> 7) & !BB_FILE_A;
    atk
}

// ---- Magic number finder ----

/// Simple PRNG for magic number candidate generation
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }

    fn sparse_random(&mut self) -> u64 {
        self.next() & self.next() & self.next()
    }
}

/// Enumerate all subsets of a mask
fn enumerate_subsets(mask: Bb) -> Vec<Bb> {
    let n = 1usize << mask.count_ones();
    let mut subsets = Vec::with_capacity(n);
    let mut subset = 0u64;
    loop {
        subsets.push(subset);
        subset = subset.wrapping_sub(mask) & mask;
        if subset == 0 { break; }
    }
    subsets
}

/// Find a valid magic number for a given square
fn find_magic(sq: Square, is_rook: bool, rng: &mut Rng) -> (u64, Vec<Bb>) {
    let mask = if is_rook { gen_rook_mask(sq) } else { gen_bishop_mask(sq) };
    let bits = mask.count_ones();
    let shift = 64 - bits;
    let size = 1usize << bits;

    // Pre-compute all occupancy -> attack mappings
    let subsets = enumerate_subsets(mask);
    let attacks: Vec<Bb> = subsets.iter().map(|&occ| {
        if is_rook { sliding_attacks_rook(sq, occ) } else { sliding_attacks_bishop(sq, occ) }
    }).collect();

    // Try random magic candidates until one works
    let mut table = vec![0u64; size];
    loop {
        let magic = rng.sparse_random();
        if ((mask.wrapping_mul(magic)) >> 56).count_ones() < 6 {
            continue;
        }

        // Clear table
        for v in table.iter_mut() { *v = 0; }

        let mut fail = false;
        for (i, &occ) in subsets.iter().enumerate() {
            let idx = (occ.wrapping_mul(magic) >> shift) as usize;
            if table[idx] == 0 {
                table[idx] = attacks[i];
            } else if table[idx] != attacks[i] {
                fail = true;
                break;
            }
        }

        if !fail {
            return (magic, table);
        }
    }
}

impl AttackTables {
    pub fn init() -> Self {
        let mut knight = [0u64; 64];
        let mut king   = [0u64; 64];
        for sq in 0..64u8 {
            knight[sq as usize] = gen_knight_attacks(sq);
            king[sq as usize]   = gen_king_attacks(sq);
        }

        let mut rng = Rng(0x12345678DEADBEEF);
        let mut rook_magics   = [MagicEntry::default(); 64];
        let mut bishop_magics = [MagicEntry::default(); 64];
        let mut rook_table:   Vec<Bb> = Vec::new();
        let mut bishop_table: Vec<Bb> = Vec::new();

        for sq in 0..64u8 {
            // Rook
            let mask = gen_rook_mask(sq);
            let bits = mask.count_ones();
            let shift = 64 - bits;
            let (magic, table) = find_magic(sq, true, &mut rng);
            let offset = rook_table.len();
            rook_table.extend_from_slice(&table);
            rook_magics[sq as usize] = MagicEntry { mask, magic, shift, offset };

            // Bishop
            let mask = gen_bishop_mask(sq);
            let bits = mask.count_ones();
            let shift = 64 - bits;
            let (magic, table) = find_magic(sq, false, &mut rng);
            let offset = bishop_table.len();
            bishop_table.extend_from_slice(&table);
            bishop_magics[sq as usize] = MagicEntry { mask, magic, shift, offset };
        }

        AttackTables {
            knight,
            king,
            rook_magics,
            bishop_magics,
            rook_table,
            bishop_table,
        }
    }

    #[inline(always)]
    pub fn rook_attacks(&self, sq: Square, occ: Bb) -> Bb {
        let entry = &self.rook_magics[sq as usize];
        let idx = ((occ & entry.mask).wrapping_mul(entry.magic) >> entry.shift) as usize;
        self.rook_table[entry.offset + idx]
    }

    #[inline(always)]
    pub fn bishop_attacks(&self, sq: Square, occ: Bb) -> Bb {
        let entry = &self.bishop_magics[sq as usize];
        let idx = ((occ & entry.mask).wrapping_mul(entry.magic) >> entry.shift) as usize;
        self.bishop_table[entry.offset + idx]
    }

    #[inline(always)]
    pub fn queen_attacks(&self, sq: Square, occ: Bb) -> Bb {
        self.rook_attacks(sq, occ) | self.bishop_attacks(sq, occ)
    }
}

// ============================================================
// Section 2: Free functions for board.rs attack detection
// ============================================================

// These use classical sliding attacks since board.rs doesn't have access to AttackTables.
// They're only used for check detection, not in the hot search path.

#[inline(always)]
pub fn knight_attacks(sq: Square) -> Bb {
    gen_knight_attacks(sq)
}

#[inline(always)]
pub fn king_attacks(sq: Square) -> Bb {
    gen_king_attacks(sq)
}

#[inline(always)]
pub fn bishop_attacks(sq: Square, occ: Bb) -> Bb {
    sliding_attacks_bishop(sq, occ)
}

#[inline(always)]
pub fn rook_attacks(sq: Square, occ: Bb) -> Bb {
    sliding_attacks_rook(sq, occ)
}

#[inline(always)]
pub fn pawn_attacks_white(pawns: Bb) -> Bb {
    ((pawns << 9) & !BB_FILE_A) | ((pawns << 7) & !BB_FILE_H)
}

#[inline(always)]
pub fn pawn_attacks_black(pawns: Bb) -> Bb {
    ((pawns >> 7) & !BB_FILE_A) | ((pawns >> 9) & !BB_FILE_H)
}

// ============================================================
// Section 3: Move list
// ============================================================

pub struct MoveList {
    pub moves:  [Move; 256],
    pub scores: [i32; 256],
    pub count:  usize,
}

impl MoveList {
    pub fn new() -> Self {
        MoveList { moves: [0; 256], scores: [0; 256], count: 0 }
    }

    #[inline(always)]
    pub fn push(&mut self, m: Move) {
        self.moves[self.count] = m;
        self.scores[self.count] = 0;
        self.count += 1;
    }

    pub fn iter(&self) -> &[Move] {
        &self.moves[..self.count]
    }

    /// Partial sort: bring best-scored move to front. O(n) per call.
    pub fn pick_best(&mut self, start: usize) {
        let mut best_idx = start;
        for i in (start + 1)..self.count {
            if self.scores[i] > self.scores[best_idx] {
                best_idx = i;
            }
        }
        self.moves.swap(start, best_idx);
        self.scores.swap(start, best_idx);
    }
}

// ============================================================
// Section 4: Move generation
// ============================================================

/// Generate all pseudo-legal moves for the side to move.
pub fn gen_moves(board: &Board, atk: &AttackTables) -> MoveList {
    let mut list = MoveList::new();
    let us   = board.side;
    let them = us.flip();
    let ui   = us as usize;
    let ti   = them as usize;
    let occ  = board.occ_all;
    let our  = board.occ[ui];
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
    let us   = board.side;
    let them = us.flip();
    let ui   = us as usize;
    let ti   = them as usize;
    let occ  = board.occ_all;
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

fn gen_pawn_moves_white(pawns: Bb, their: Bb, ep_sq: Square, occ: Bb, list: &mut MoveList) {
    let single = (pawns << 8) & !occ;
    let promo  = single & BB_RANK_8;
    let normal = single & !BB_RANK_8;
    add_pawn_moves(normal, -8i32, list);
    add_pawn_promos(promo, -8i32, list);

    let double = ((single & BB_RANK_3) << 8) & !occ;
    add_pawn_moves(double, -16i32, list);

    let cap_left  = (pawns << 9) & !BB_FILE_A & their;
    let cap_right = (pawns << 7) & !BB_FILE_H & their;
    add_pawn_moves(cap_left  & !BB_RANK_8, -9i32, list);
    add_pawn_moves(cap_right & !BB_RANK_8, -7i32, list);
    add_pawn_promos(cap_left  & BB_RANK_8, -9i32, list);
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

fn gen_pawn_moves_black(pawns: Bb, their: Bb, ep_sq: Square, occ: Bb, list: &mut MoveList) {
    let single = (pawns >> 8) & !occ;
    let promo  = single & BB_RANK_1;
    let normal = single & !BB_RANK_1;
    add_pawn_moves(normal, 8i32, list);
    add_pawn_promos(promo, 8i32, list);

    let double = ((single & BB_RANK_6) >> 8) & !occ;
    add_pawn_moves(double, 16i32, list);

    let cap_left  = (pawns >> 7) & !BB_FILE_A & their;
    let cap_right = (pawns >> 9) & !BB_FILE_H & their;
    add_pawn_moves(cap_left  & !BB_RANK_1, 7i32, list);
    add_pawn_moves(cap_right & !BB_RANK_1, 9i32, list);
    add_pawn_promos(cap_left  & BB_RANK_1, 7i32, list);
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

fn gen_pawn_captures_white(pawns: Bb, their: Bb, ep_sq: Square, occ: Bb, list: &mut MoveList) {
    let cap_left  = (pawns << 9) & !BB_FILE_A & their;
    let cap_right = (pawns << 7) & !BB_FILE_H & their;
    // Push-promotions require an EMPTY target square. Without the !occ mask
    // a blocked pawn "promotes onto" the blocker — and when the blocker is
    // the enemy king, make_move captures the king and corrupts the board.
    add_pawn_promos((pawns << 8) & BB_RANK_8 & !occ, -8i32, list);
    add_pawn_promos(cap_left  & BB_RANK_8, -9i32, list);
    add_pawn_promos(cap_right & BB_RANK_8, -7i32, list);
    add_pawn_moves(cap_left  & !BB_RANK_8, -9i32, list);
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

fn gen_pawn_captures_black(pawns: Bb, their: Bb, ep_sq: Square, occ: Bb, list: &mut MoveList) {
    let cap_left  = (pawns >> 7) & !BB_FILE_A & their;
    let cap_right = (pawns >> 9) & !BB_FILE_H & their;
    add_pawn_promos((pawns >> 8) & BB_RANK_1 & !occ, 8i32, list);
    add_pawn_promos(cap_left  & BB_RANK_1, 7i32, list);
    add_pawn_promos(cap_right & BB_RANK_1, 9i32, list);
    add_pawn_moves(cap_left  & !BB_RANK_1, 7i32, list);
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

fn add_pawn_moves(mut targets: Bb, from_delta: i32, list: &mut MoveList) {
    while targets != 0 {
        let to = bb_pop_lsb(&mut targets);
        let from = (to as i32 + from_delta) as Square;
        list.push(make_move(from, to));
    }
}

fn add_pawn_promos(mut targets: Bb, from_delta: i32, list: &mut MoveList) {
    while targets != 0 {
        let to = bb_pop_lsb(&mut targets);
        let from = (to as i32 + from_delta) as Square;
        list.push(make_move_promo(from, to, PieceType::Queen));
        list.push(make_move_promo(from, to, PieceType::Rook));
        list.push(make_move_promo(from, to, PieceType::Bishop));
        list.push(make_move_promo(from, to, PieceType::Knight));
    }
}

fn add_moves(from: Square, mut targets: Bb, list: &mut MoveList) {
    while targets != 0 {
        let to = bb_pop_lsb(&mut targets);
        list.push(make_move(from, to));
    }
}

fn gen_castling(board: &Board, king_sq: Square, us: Color, occ: Bb, list: &mut MoveList) {
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

/// Generate quiet (non-capture) moves that give check.
/// Used in quiescence search to find tactical checks.
/// Only generates knight and discovered checks for efficiency.
pub fn gen_quiet_checks(board: &Board, atk: &AttackTables) -> MoveList {
    let mut list = MoveList::new();
    let us = board.side;
    let ui = us as usize;
    let ti = us.flip() as usize;
    let occ = board.occ_all;
    let _our = board.occ[ui];
    let their_king_sq = board.king_sq[ti];
    if their_king_sq == NO_SQUARE { return list; }

    // Knight checks: knight moves to a square that attacks the enemy king
    let king_atk = atk.knight[their_king_sq as usize];
    let mut knights = board.pieces[ui][PieceType::Knight as usize];
    while knights != 0 {
        let from = bb_pop_lsb(&mut knights);
        // Non-capture moves that land on a square attacking the enemy king
        let targets = atk.knight[from as usize] & !occ & king_atk;
        add_moves(from, targets, &mut list);
    }

    // Bishop/Queen discovered checks via rook movement are complex.
    // For simplicity, only generate direct piece checks:

    // Bishop checks: bishop moves to a square on enemy king's diagonals
    let king_bishop_rays = atk.bishop_attacks(their_king_sq, occ);
    let mut bishops = board.pieces[ui][PieceType::Bishop as usize];
    while bishops != 0 {
        let from = bb_pop_lsb(&mut bishops);
        let targets = atk.bishop_attacks(from, occ) & !occ & king_bishop_rays;
        add_moves(from, targets, &mut list);
    }

    // Rook checks: rook moves to a square on enemy king's rank/file
    let king_rook_rays = atk.rook_attacks(their_king_sq, occ);
    let mut rooks = board.pieces[ui][PieceType::Rook as usize];
    while rooks != 0 {
        let from = bb_pop_lsb(&mut rooks);
        let targets = atk.rook_attacks(from, occ) & !occ & king_rook_rays;
        add_moves(from, targets, &mut list);
    }

    // Queen checks: queen moves that directly check the king
    let king_queen_rays = king_bishop_rays | king_rook_rays;
    let mut queens = board.pieces[ui][PieceType::Queen as usize];
    while queens != 0 {
        let from = bb_pop_lsb(&mut queens);
        let targets = atk.queen_attacks(from, occ) & !occ & king_queen_rays;
        add_moves(from, targets, &mut list);
    }

    list
}


// ============================================================
// Section 4b: Static Exchange Evaluation (SEE)
// ============================================================

/// SEE piece values — simpler than eval values, just for exchange calculation
const SEE_VALUES: [i32; 7] = [
    100,   // Pawn
    320,   // Knight
    330,   // Bishop
    500,   // Rook
    900,   // Queen
    20000, // King
    0,     // None
];

/// Get the least valuable attacker of a square for the given side.
/// Returns the piece type and removes it from the `occ` and piece bitboards.
fn least_valuable_attacker(
    board: &Board,
    atk: &AttackTables,
    to: Square,
    side: Color,
    occ: &mut Bb,
) -> PieceType {
    let ci = side as usize;

    // Pawns
    let pawn_attackers = if side == Color::White {
        // White pawns attack from below: to-7 (left) and to-9 (right)
        let mut mask = 0u64;
        if to >= 9 && sq_file(to) > 0 { mask |= bb(to - 9); }
        if to >= 7 && sq_file(to) < 7 { mask |= bb(to - 7); }
        mask
    } else {
        // Black pawns attack from above: to+7 (left) and to+9 (right)
        let mut mask = 0u64;
        if to <= 56 && sq_file(to) > 0 { mask |= bb(to + 7); }
        if to <= 54 && sq_file(to) < 7 { mask |= bb(to + 9); }
        mask
    };
    let pawns = pawn_attackers & board.pieces[ci][PieceType::Pawn as usize] & *occ;
    if pawns != 0 {
        *occ ^= bb(bb_lsb(pawns));
        return PieceType::Pawn;
    }

    // Knights
    let knights = atk.knight[to as usize] & board.pieces[ci][PieceType::Knight as usize] & *occ;
    if knights != 0 {
        *occ ^= bb(bb_lsb(knights));
        return PieceType::Knight;
    }

    // Bishops
    let bishop_atk = atk.bishop_attacks(to, *occ);
    let bishops = bishop_atk & board.pieces[ci][PieceType::Bishop as usize] & *occ;
    if bishops != 0 {
        *occ ^= bb(bb_lsb(bishops));
        return PieceType::Bishop;
    }

    // Rooks
    let rook_atk = atk.rook_attacks(to, *occ);
    let rooks = rook_atk & board.pieces[ci][PieceType::Rook as usize] & *occ;
    if rooks != 0 {
        *occ ^= bb(bb_lsb(rooks));
        return PieceType::Rook;
    }

    // Queens
    let queen_atk = bishop_atk | rook_atk;
    let queens = queen_atk & board.pieces[ci][PieceType::Queen as usize] & *occ;
    if queens != 0 {
        *occ ^= bb(bb_lsb(queens));
        return PieceType::Queen;
    }

    // King
    let kings = atk.king[to as usize] & board.pieces[ci][PieceType::King as usize] & *occ;
    if kings != 0 {
        *occ ^= bb(bb_lsb(kings));
        return PieceType::King;
    }

    PieceType::None
}

/// Static Exchange Evaluation (SEE)
///
/// Determines the material outcome of a capture exchange on the target square.
/// Returns the net material gain from the perspective of the side making the move.
///
/// A positive return means the capture sequence is winning.
/// A negative return means the capture sequence is losing.
/// Zero means an equal exchange.
pub fn see(board: &Board, atk: &AttackTables, m: Move) -> i32 {
    let from = move_from(m);
    let to = move_to(m);

    let mover_piece = board.sq_piece[from as usize];
    if mover_piece == PIECE_NONE { return 0; }
    let mover_pt = piece_type(mover_piece);
    let mover_color = piece_color(mover_piece);

    // Initial captured piece value
    let cap_piece = board.sq_piece[to as usize];
    let mut gain = if cap_piece != PIECE_NONE {
        SEE_VALUES[piece_type(cap_piece) as usize]
    } else if move_flags(m) == MF_EN_PASSANT {
        SEE_VALUES[PieceType::Pawn as usize]
    } else {
        0 // quiet move — SEE = 0 for non-captures
    };

    // For promotions, add the promotion piece value minus pawn value
    if move_flags(m) == MF_PROMOTION {
        let promo_pt = move_promo_pt(m);
        gain += SEE_VALUES[promo_pt as usize] - SEE_VALUES[PieceType::Pawn as usize];
    }

    // Swap list: track gains at each recapture depth
    let mut gains = [0i32; 32];
    gains[0] = gain;
    let mut depth = 0usize;

    // Remove the initial mover from occupancy
    let mut occ = board.occ_all ^ bb(from);
    // For en passant, also remove the captured pawn
    if move_flags(m) == MF_EN_PASSANT {
        let ep_cap_sq = if mover_color == Color::White { to - 8 } else { to + 8 };
        occ ^= bb(ep_cap_sq);
    }

    let mut current_attacker_value = if move_flags(m) == MF_PROMOTION {
        SEE_VALUES[move_promo_pt(m) as usize]
    } else {
        SEE_VALUES[mover_pt as usize]
    };
    let mut side = mover_color.flip();

    loop {
        depth += 1;
        if depth >= 32 { break; }

        // Negamax: the gain at this depth is the value of recapturing
        // minus whatever the opponent gained
        gains[depth] = current_attacker_value - gains[depth - 1];

        // Pruning: if the side to recapture can't improve their position
        // even by capturing, they won't recapture (stand pat)
        if (-gains[depth]).max(gains[depth - 1]) < 0 {
            break;
        }

        // Find least valuable attacker for this side
        let pt = least_valuable_attacker(board, atk, to, side, &mut occ);
        if pt == PieceType::None { break; }

        current_attacker_value = SEE_VALUES[pt as usize];
        side = side.flip();
    }

    // Minimax the gains: unwind from the deepest capture
    while depth > 0 {
        depth -= 1;
        gains[depth] = -((-gains[depth]).max(gains[depth + 1]));
    }

    gains[0]
}

/// Quick SEE threshold test: returns true if SEE >= threshold.
/// Slightly more efficient than computing full SEE when you only need a boolean.
#[inline]
pub fn see_ge(board: &Board, atk: &AttackTables, m: Move, threshold: i32) -> bool {
    see(board, atk, m) >= threshold
}


// ============================================================
// Section 5: Perft (for correctness verification)
// ============================================================

pub fn perft(board: &mut Board, atk: &AttackTables, z: &crate::board::Zobrist, depth: u32) -> u64 {
    if depth == 0 { return 1; }
    let list = gen_moves(board, atk);
    let mut nodes = 0u64;
    for i in 0..list.count {
        let m = list.moves[i];
        let undo = board.make_move(m, z);
        if !board.is_in_check(board.side.flip()) {
            nodes += perft(board, atk, z, depth - 1);
        }
        board.unmake_move(m, &undo, z);
    }
    nodes
}
