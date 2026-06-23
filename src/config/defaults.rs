use super::*;
impl Default for EngineOptions {
    fn default() -> Self {
        EngineOptions {
            eval: EvalOptions::default(),
            search: SearchOptions::default(),
            syzygy: SyzygyOptions::default(),
            criticality: CriticalityOptions::default(),
        }
    }
}

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
            see_capture_ordering: true,
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

impl Default for CriticalityOptions {
    fn default() -> Self {
        let log_dir = std::env::var("BOA_CRITICALITY_LOG_DIR").unwrap_or_default();
        let probe_permille = std::env::var("BOA_CRITICALITY_PROBE_PERMILLE")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0)
            .clamp(0, 1000);
        let futility_probe_permille = std::env::var("BOA_FUTILITY_PROBE_PERMILLE")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(probe_permille)
            .clamp(0, 1000);
        CriticalityOptions {
            log_dir,
            probe_permille,
            futility_probe_permille,
        }
    }
}
