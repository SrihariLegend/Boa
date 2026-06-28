// ============================================================

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EngineOptions {
    pub eval: EvalOptions,
    pub search: SearchOptions,
    pub syzygy: SyzygyOptions,
    pub criticality: CriticalityOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvalOptions {
    pub material_scale: i32,
    pub pst_scale: i32,
    pub mobility_scale: i32,
    pub pawn_structure_scale: i32,
    pub king_safety_scale: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchOptions {
    pub threads: usize,
    pub lazy_smp: bool,
    pub see: bool,
    pub see_qsearch_pruning: bool,
    pub see_capture_ordering: bool,
    pub forward_futility_pruning: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyzygyOptions {
    pub path: String,
    pub probe_depth: u32,
    pub probe_limit: usize,
    pub fifty_move_rule: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CriticalityOptions {
    pub log_dir: String,
    pub probe_permille: u32,
    pub futility_probe_permille: u32,
}
