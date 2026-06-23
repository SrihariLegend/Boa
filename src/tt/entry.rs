use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Bound {
    None = 0,
    Exact = 1,
    Lower = 2,
    Upper = 3,
}

#[derive(Clone, Copy)]
pub struct TtEntry {
    pub key: u32,
    pub score: i32,
    pub best: Move,
    pub depth: i8,
    pub bound: Bound,
    pub age: u8,
}
