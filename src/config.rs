// ============================================================
// config.rs - UCI-controlled engine feature toggles
// ============================================================

#[derive(Clone, Debug, PartialEq, Eq)]
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
}

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
        CriticalityOptions {
            log_dir,
            probe_permille,
        }
    }
}

impl EngineOptions {
    pub fn set_uci_option(&mut self, name: &str, value: &str) -> bool {
        let key = normalize_option_name(name);
        match key.as_str() {
            "threads" | "searchthreads" => set_threads(&mut self.search.threads, value),
            "searchlazysmp" => set_bool(&mut self.search.lazy_smp, value),
            "evalmaterialscale" => set_scale(&mut self.eval.material_scale, value),
            "evalpstscale" => set_scale(&mut self.eval.pst_scale, value),
            "evalmobilityscale" => set_scale(&mut self.eval.mobility_scale, value),
            "evalpawnstructurescale" => set_scale(&mut self.eval.pawn_structure_scale, value),
            "evalkingsafetyscale" => set_scale(&mut self.eval.king_safety_scale, value),
            "searchsee" => set_bool(&mut self.search.see, value),
            "searchseeqsearchpruning" => set_bool(&mut self.search.see_qsearch_pruning, value),
            "searchseecaptureordering" => set_bool(&mut self.search.see_capture_ordering, value),
            "syzygypath" => {
                self.syzygy.path = value.to_string();
                true
            }
            "syzygyprobedepth" => set_u32(&mut self.syzygy.probe_depth, value, 0, 64),
            "syzygyprobelimit" => set_usize(&mut self.syzygy.probe_limit, value, 0, 6),
            "syzygy50moverule" => set_bool(&mut self.syzygy.fifty_move_rule, value),
            "criticalitylogdir" => {
                self.criticality.log_dir = value.to_string();
                true
            }
            "criticalityprobepermille" => {
                set_u32(&mut self.criticality.probe_permille, value, 0, 1000)
            }
            _ => false,
        }
    }
}

fn normalize_option_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_ascii_whitespace() && *c != '_' && *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

fn set_scale(target: &mut i32, value: &str) -> bool {
    let Ok(scale) = value.parse::<i32>() else {
        return false;
    };
    *target = scale.clamp(0, 300);
    true
}

fn set_threads(target: &mut usize, value: &str) -> bool {
    set_usize(target, value, 1, 64)
}

fn set_usize(target: &mut usize, value: &str, min: usize, max: usize) -> bool {
    let Ok(parsed) = value.parse::<usize>() else {
        return false;
    };
    *target = parsed.clamp(min, max);
    true
}

fn set_u32(target: &mut u32, value: &str, min: u32, max: u32) -> bool {
    let Ok(parsed) = value.parse::<u32>() else {
        return false;
    };
    *target = parsed.clamp(min, max);
    true
}

fn set_bool(target: &mut bool, value: &str) -> bool {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => {
            *target = true;
            true
        }
        "false" | "0" | "no" | "off" => {
            *target = false;
            true
        }
        _ => false,
    }
}

pub fn scale_score(score: i32, scale: i32) -> i32 {
    score * scale / 100
}

pub fn scale_score_pair(score: (i32, i32), scale: i32) -> (i32, i32) {
    (scale_score(score.0, scale), scale_score(score.1, scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uci_option_names_accept_spaces_and_case() {
        let mut options = EngineOptions::default();

        assert!(options.set_uci_option("Threads", "4"));
        assert_eq!(options.search.threads, 4);

        assert!(options.set_uci_option("Search Lazy SMP", "false"));
        assert!(!options.search.lazy_smp);

        assert!(options.set_uci_option("Search SEE QSearch Pruning", "false"));
        assert!(!options.search.see_qsearch_pruning);

        assert!(options.set_uci_option("Search SEE Capture Ordering", "false"));
        assert!(!options.search.see_capture_ordering);

        assert!(options.set_uci_option("Syzygy Path", "/tmp/tb"));
        assert_eq!(options.syzygy.path, "/tmp/tb");

        assert!(options.set_uci_option("Criticality Log Dir", "/tmp/criticality"));
        assert_eq!(options.criticality.log_dir, "/tmp/criticality");
    }

    #[test]
    fn eval_scales_are_clamped() {
        let mut options = EngineOptions::default();

        assert!(options.set_uci_option("Eval Mobility Scale", "999"));
        assert_eq!(options.eval.mobility_scale, 300);

        assert!(options.set_uci_option("Eval Mobility Scale", "-10"));
        assert_eq!(options.eval.mobility_scale, 0);

        assert!(options.set_uci_option("Syzygy Probe Limit", "99"));
        assert_eq!(options.syzygy.probe_limit, 6);

        assert!(options.set_uci_option("Criticality Probe Permille", "2000"));
        assert_eq!(options.criticality.probe_permille, 1000);
    }
}
