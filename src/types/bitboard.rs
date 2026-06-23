use super::*;
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
    BB_FILE_A, BB_FILE_B, BB_FILE_C, BB_FILE_D, BB_FILE_E, BB_FILE_F, BB_FILE_G, BB_FILE_H,
];
pub const BB_RANKS: [Bb; 8] = [
    BB_RANK_1, BB_RANK_2, BB_RANK_3, BB_RANK_4, BB_RANK_5, BB_RANK_6, BB_RANK_7, BB_RANK_8,
];
pub const BB_LIGHT_SQUARES: Bb = 0x55AA55AA55AA55AA;
pub const BB_DARK_SQUARES: Bb = 0xAA55AA55AA55AA55;
pub const BB_CENTER: Bb = (1u64 << D4) | (1u64 << E4) | (1u64 << D5) | (1u64 << E5);
pub const BB_EXTENDED_CENTER: Bb = (1u64 << C3)
    | (1u64 << D3)
    | (1u64 << E3)
    | (1u64 << F3)
    | (1u64 << C4)
    | (1u64 << D4)
    | (1u64 << E4)
    | (1u64 << F4)
    | (1u64 << C5)
    | (1u64 << D5)
    | (1u64 << E5)
    | (1u64 << F5)
    | (1u64 << C6)
    | (1u64 << D6)
    | (1u64 << E6)
    | (1u64 << F6);

#[inline(always)]
pub fn bb(sq: Square) -> Bb {
    1u64 << sq
}

#[inline(always)]
pub fn bb_lsb(b: Bb) -> Square {
    b.trailing_zeros() as Square
}

#[inline(always)]
pub fn bb_pop_lsb(b: &mut Bb) -> Square {
    let sq = bb_lsb(*b);
    *b &= *b - 1;
    sq
}

#[inline(always)]
pub fn bb_popcount(b: Bb) -> u32 {
    b.count_ones()
}
