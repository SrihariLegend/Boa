// ---- Castling rights bitmask ----
pub const CR_WHITE_KINGSIDE: u8 = 1;
pub const CR_WHITE_QUEENSIDE: u8 = 2;
pub const CR_BLACK_KINGSIDE: u8 = 4;
pub const CR_BLACK_QUEENSIDE: u8 = 8;
pub const CR_WHITE: u8 = CR_WHITE_KINGSIDE | CR_WHITE_QUEENSIDE;
pub const CR_BLACK: u8 = CR_BLACK_KINGSIDE | CR_BLACK_QUEENSIDE;
