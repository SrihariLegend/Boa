use super::*;
// ============================================================
// Section 1: Attack tables
// ============================================================

pub struct AttackTables {
    pub knight: [Bb; 64],
    pub king: [Bb; 64],
    rook_magics: [MagicEntry; 64],
    bishop_magics: [MagicEntry; 64],
    rook_table: Vec<Bb>,
    bishop_table: Vec<Bb>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct MagicEntry {
    pub(super) mask: Bb,
    pub(super) magic: u64,
    pub(super) shift: u32,
    pub(super) offset: usize,
}

// ---- Mask generation ----

pub(super) fn gen_rook_mask(sq: Square) -> Bb {
    let r = sq_rank(sq) as i32;
    let f = sq_file(sq) as i32;
    let mut mask = 0u64;
    for i in (f + 1)..7 {
        mask |= 1u64 << (r * 8 + i);
    }
    for i in 1..f {
        mask |= 1u64 << (r * 8 + i);
    }
    for i in (r + 1)..7 {
        mask |= 1u64 << (i * 8 + f);
    }
    for i in 1..r {
        mask |= 1u64 << (i * 8 + f);
    }
    mask
}

pub(super) fn gen_bishop_mask(sq: Square) -> Bb {
    let r = sq_rank(sq) as i32;
    let f = sq_file(sq) as i32;
    let mut mask = 0u64;
    for (dr, df) in [(1i32, 1i32), (1, -1), (-1, 1), (-1, -1)] {
        let mut cr = r + dr;
        let mut cf = f + df;
        while cr > 0 && cr < 7 && cf > 0 && cf < 7 {
            mask |= 1u64 << (cr * 8 + cf);
            cr += dr;
            cf += df;
        }
    }
    mask
}

// ---- Classical sliding attacks (used for table generation and fallback) ----

pub(super) fn sliding_attacks_rook(sq: Square, occ: Bb) -> Bb {
    let r = sq_rank(sq) as i32;
    let f = sq_file(sq) as i32;
    let mut atk = 0u64;
    for (dr, df) in [(0i32, 1i32), (0, -1), (1, 0), (-1, 0)] {
        let mut cr = r + dr;
        let mut cf = f + df;
        while (0..=7).contains(&cr) && (0..=7).contains(&cf) {
            let s = (cr * 8 + cf) as u32;
            atk |= 1u64 << s;
            if occ & (1u64 << s) != 0 {
                break;
            }
            cr += dr;
            cf += df;
        }
    }
    atk
}

pub(super) fn sliding_attacks_bishop(sq: Square, occ: Bb) -> Bb {
    let r = sq_rank(sq) as i32;
    let f = sq_file(sq) as i32;
    let mut atk = 0u64;
    for (dr, df) in [(1i32, 1i32), (1, -1), (-1, 1), (-1, -1)] {
        let mut cr = r + dr;
        let mut cf = f + df;
        while (0..=7).contains(&cr) && (0..=7).contains(&cf) {
            let s = (cr * 8 + cf) as u32;
            atk |= 1u64 << s;
            if occ & (1u64 << s) != 0 {
                break;
            }
            cr += dr;
            cf += df;
        }
    }
    atk
}

// ---- Non-slider attack generation ----

pub(super) fn gen_knight_attacks(sq: Square) -> Bb {
    let bb_sq = bb(sq);
    let mut atk = 0u64;
    atk |= (bb_sq << 17) & !BB_FILE_A;
    atk |= (bb_sq << 15) & !BB_FILE_H;
    atk |= (bb_sq << 10) & !(BB_FILE_A | BB_FILE_B);
    atk |= (bb_sq << 6) & !(BB_FILE_G | BB_FILE_H);
    atk |= (bb_sq >> 17) & !BB_FILE_H;
    atk |= (bb_sq >> 15) & !BB_FILE_A;
    atk |= (bb_sq >> 10) & !(BB_FILE_G | BB_FILE_H);
    atk |= (bb_sq >> 6) & !(BB_FILE_A | BB_FILE_B);
    atk
}

pub(super) fn gen_king_attacks(sq: Square) -> Bb {
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
pub(super) struct Rng(u64);

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
pub(super) fn enumerate_subsets(mask: Bb) -> Vec<Bb> {
    let n = 1usize << mask.count_ones();
    let mut subsets = Vec::with_capacity(n);
    let mut subset = 0u64;
    loop {
        subsets.push(subset);
        subset = subset.wrapping_sub(mask) & mask;
        if subset == 0 {
            break;
        }
    }
    subsets
}

/// Find a valid magic number for a given square
pub(super) fn find_magic(sq: Square, is_rook: bool, rng: &mut Rng) -> (u64, Vec<Bb>) {
    let mask = if is_rook {
        gen_rook_mask(sq)
    } else {
        gen_bishop_mask(sq)
    };
    let bits = mask.count_ones();
    let shift = 64 - bits;
    let size = 1usize << bits;

    // Pre-compute all occupancy -> attack mappings
    let subsets = enumerate_subsets(mask);
    let attacks: Vec<Bb> = subsets
        .iter()
        .map(|&occ| {
            if is_rook {
                sliding_attacks_rook(sq, occ)
            } else {
                sliding_attacks_bishop(sq, occ)
            }
        })
        .collect();

    // Try random magic candidates until one works
    let mut table = vec![0u64; size];
    loop {
        let magic = rng.sparse_random();
        if ((mask.wrapping_mul(magic)) >> 56).count_ones() < 6 {
            continue;
        }

        // Clear table
        for v in table.iter_mut() {
            *v = 0;
        }

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
        let mut king = [0u64; 64];
        for sq in 0..64u8 {
            knight[sq as usize] = gen_knight_attacks(sq);
            king[sq as usize] = gen_king_attacks(sq);
        }

        let mut rng = Rng(0x12345678DEADBEEF);
        let mut rook_magics = [MagicEntry::default(); 64];
        let mut bishop_magics = [MagicEntry::default(); 64];
        let mut rook_table: Vec<Bb> = Vec::new();
        let mut bishop_table: Vec<Bb> = Vec::new();

        for sq in 0..64u8 {
            // Rook
            let mask = gen_rook_mask(sq);
            let bits = mask.count_ones();
            let shift = 64 - bits;
            let (magic, table) = find_magic(sq, true, &mut rng);
            let offset = rook_table.len();
            rook_table.extend_from_slice(&table);
            rook_magics[sq as usize] = MagicEntry {
                mask,
                magic,
                shift,
                offset,
            };

            // Bishop
            let mask = gen_bishop_mask(sq);
            let bits = mask.count_ones();
            let shift = 64 - bits;
            let (magic, table) = find_magic(sq, false, &mut rng);
            let offset = bishop_table.len();
            bishop_table.extend_from_slice(&table);
            bishop_magics[sq as usize] = MagicEntry {
                mask,
                magic,
                shift,
                offset,
            };
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
