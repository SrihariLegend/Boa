use super::*;
pub fn should_probe(
    hash: u64,
    m: Move,
    depth: i32,
    ply: usize,
    search_id: u64,
    permille: u32,
) -> bool {
    if permille == 0 {
        return false;
    }
    if permille >= 1000 {
        return true;
    }
    criticality_sample_bucket(hash, m, depth, ply, search_id) < permille
}

pub fn criticality_sample_bucket(
    hash: u64,
    m: Move,
    depth: i32,
    ply: usize,
    search_id: u64,
) -> u32 {
    let mut x = hash
        ^ ((m as u64) << 17)
        ^ ((depth as u64) << 41)
        ^ ((ply as u64) << 53)
        ^ search_id.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x % 1000) as u32
}

pub(super) fn bool_int(value: bool) -> String {
    if value {
        "1".to_string()
    } else {
        "0".to_string()
    }
}

pub(super) fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "white",
        Color::Black => "black",
    }
}
