use super::*;
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
            "searchforwardfutilitypruning" => {
                set_bool(&mut self.search.forward_futility_pruning, value)
            }
            "syzygypath" => {
                self.syzygy.path = value.to_string();
                true
            }
            "syzygyprobedepth" => set_u32(&mut self.syzygy.probe_depth, value, 0, 64),
            "syzygyprobelimit" => set_usize(&mut self.syzygy.probe_limit, value, 0, 6),
            "syzygy50moverule" => set_bool(&mut self.syzygy.fifty_move_rule, value),
            _ => false,
        }
    }
}

pub(super) fn normalize_option_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_ascii_whitespace() && *c != '_' && *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn set_scale(target: &mut i32, value: &str) -> bool {
    let Ok(scale) = value.parse::<i32>() else {
        return false;
    };
    *target = scale.clamp(0, 300);
    true
}

pub(super) fn set_threads(target: &mut usize, value: &str) -> bool {
    set_usize(target, value, 1, 64)
}

pub(super) fn set_usize(target: &mut usize, value: &str, min: usize, max: usize) -> bool {
    let Ok(parsed) = value.parse::<usize>() else {
        return false;
    };
    *target = parsed.clamp(min, max);
    true
}

pub(super) fn set_u32(target: &mut u32, value: &str, min: u32, max: u32) -> bool {
    let Ok(parsed) = value.parse::<u32>() else {
        return false;
    };
    *target = parsed.clamp(min, max);
    true
}

pub(super) fn set_bool(target: &mut bool, value: &str) -> bool {
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
