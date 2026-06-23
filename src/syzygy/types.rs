use super::*;
pub struct SyzygyTablebase {
    pub(super) tables: Tablebase<Chess>,
    pub(super) files: usize,
}

pub struct SyzygyRootProbe {
    pub best_move: Move,
    pub score: Score,
    pub wdl: String,
    pub dtz: i32,
}
