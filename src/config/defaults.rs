use super::*;

impl Default for EvalOptions {
    fn default() -> Self {
        EvalOptions {
            material_scale: 100,
            pst_scale: 100,
            mobility_scale: 100,
            pawn_structure_scale: 100,
            king_safety_scale: 100,
        }
    }
}

impl Default for SearchOptions {
    fn default() -> Self {
        SearchOptions {
            threads: 1,
            lazy_smp: true,
            see: true,
            see_qsearch_pruning: true,
            forward_futility_pruning: true,
        }
    }
}

impl Default for SyzygyOptions {
    fn default() -> Self {
        SyzygyOptions {
            path: String::new(),
            probe_depth: 1,
            probe_limit: 6,
            fifty_move_rule: true,
        }
    }
}

