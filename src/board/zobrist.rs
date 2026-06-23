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
        for color_entries in &mut piece_sq {
            for piece_entries in color_entries {
                for square_entry in piece_entries {
                    *square_entry = next();
                }
            }
        }
        let side = next();
        let mut castling = [0u64; 16];
        for entry in &mut castling {
            *entry = next();
        }
        let mut ep_file = [0u64; 8];
        for entry in &mut ep_file {
            *entry = next();
        }
        Zobrist {
            piece_sq,
            side,
            castling,
            ep_file,
        }
    }
}
