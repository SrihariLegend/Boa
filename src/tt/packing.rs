use super::*;
pub(super) fn pack_ctrl(key: u32, depth: i8, bound: Bound, age: u16) -> u64 {
    (key as u64)
        | ((depth as u8 as u64) << 32)
        | ((bound as u8 as u64) << 40)
        | ((age as u64) << 48)
}

/// Pack score (i32), best move (u16), and raw_eval (i16) into a 64-bit word.
/// Layout: bits 63-48 = raw_eval, bits 47-32 = best_move, bits 31-0 = score
pub(super) fn pack_data(score: Score, best: Move, raw_eval: i16) -> u64 {
    (score as u32 as u64) | ((best as u64) << 32) | ((raw_eval as u16 as u64) << 48)
}

pub(super) fn unpack_entry(ctrl: u64, data: u64) -> TtEntry {
    let bound = match ((ctrl >> 40) & 0xFF) as u8 {
        1 => Bound::Exact,
        2 => Bound::Lower,
        3 => Bound::Upper,
        _ => Bound::None,
    };
    TtEntry {
        key: ctrl as u32,
        score: data as u32 as i32,
        best: ((data >> 32) & 0xFFFF) as Move,
        raw_eval: ((data >> 48) & 0xFFFF) as u16 as i16,
        depth: ((ctrl >> 32) & 0xFF) as u8 as i8,
        bound,
        age: ((ctrl >> 48) & 0xFFFF) as u16,
    }
}
