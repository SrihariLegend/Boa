use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Bound {
    None = 0,
    Exact = 1,
    Lower = 2,
    Upper = 3,
}

pub fn bound_str(bound: Bound) -> &'static str {
    match bound {
        Bound::None => "none",
        Bound::Exact => "exact",
        Bound::Lower => "lower",
        Bound::Upper => "upper",
    }
}

#[derive(Clone, Copy)]
pub struct TtEntry {
    pub key: u32,
    pub score: i32,
    pub best: Move,
    pub depth: i8,
    pub bound: Bound,
    pub age: u16,
    /// Raw (uncorrected) static evaluation. Zero means "not stored".
    /// When non-zero, the search can reuse this instead of calling evaluate().
    pub raw_eval: i16,
}
