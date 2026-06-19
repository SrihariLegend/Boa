// ============================================================
// config.rs - UCI-controlled engine feature toggles
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineOptions {
    pub eval: EvalOptions,
    pub search: SearchOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvalOptions {
    pub material_scale: i32,
    pub pst_scale: i32,
    pub mobility_scale: i32,
    pub pawn_structure_scale: i32,
    pub king_safety_scale: i32,
    pub freedom_scale: i32,
    pub trade_down_scale: i32,
    pub weak_squares_scale: i32,
    pub coordination_scale: i32,
    pub advanced_pawns_scale: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchOptions {
    pub restriction_ordering: bool,
    pub restriction_ordering_scale: i32,
    pub squeeze_extensions: bool,
    pub squeeze_null_move_suppression: bool,
    pub squeeze_lmr_relief: bool,
}

impl Default for EngineOptions {
    fn default() -> Self {
        EngineOptions {
            eval: EvalOptions::default(),
            search: SearchOptions::default(),
        }
    }
}

impl Default for EvalOptions {
    fn default() -> Self {
        EvalOptions {
            material_scale: 108,
            pst_scale: 101,
            mobility_scale: 103,
            pawn_structure_scale: 98,
            king_safety_scale: 101,
            freedom_scale: 101,
            trade_down_scale: 102,
            weak_squares_scale: 100,
            coordination_scale: 101,
            advanced_pawns_scale: 100,
        }
    }
}

impl Default for SearchOptions {
    fn default() -> Self {
        SearchOptions {
            restriction_ordering: true,
            restriction_ordering_scale: 100,
            squeeze_extensions: true,
            squeeze_null_move_suppression: true,
            squeeze_lmr_relief: true,
        }
    }
}

impl EngineOptions {
    pub fn set_uci_option(&mut self, name: &str, value: &str) -> bool {
        let key = normalize_option_name(name);
        match key.as_str() {
            "evalmaterialscale" => set_scale(&mut self.eval.material_scale, value),
            "evalpstscale" => set_scale(&mut self.eval.pst_scale, value),
            "evalmobilityscale" => set_scale(&mut self.eval.mobility_scale, value),
            "evalpawnstructurescale" => set_scale(&mut self.eval.pawn_structure_scale, value),
            "evalkingsafetyscale" => set_scale(&mut self.eval.king_safety_scale, value),
            "evalfreedomscale" => set_scale(&mut self.eval.freedom_scale, value),
            "evaltradedownscale" => set_scale(&mut self.eval.trade_down_scale, value),
            "evalweaksquaresscale" => set_scale(&mut self.eval.weak_squares_scale, value),
            "evalcoordinationscale" => set_scale(&mut self.eval.coordination_scale, value),
            "evaladvancedpawnsscale" => set_scale(&mut self.eval.advanced_pawns_scale, value),
            "searchrestrictionordering" => set_bool(&mut self.search.restriction_ordering, value),
            "searchrestrictionorderingscale" => {
                set_scale(&mut self.search.restriction_ordering_scale, value)
            }
            "searchsqueezeextensions" => set_bool(&mut self.search.squeeze_extensions, value),
            "searchsqueezenullmovesuppression" => {
                set_bool(&mut self.search.squeeze_null_move_suppression, value)
            }
            "searchsqueezelmrrelief" => set_bool(&mut self.search.squeeze_lmr_relief, value),
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

        assert!(options.set_uci_option("Eval Freedom Scale", "0"));
        assert_eq!(options.eval.freedom_scale, 0);

        assert!(options.set_uci_option("search-squeeze-null-move-suppression", "false"));
        assert!(!options.search.squeeze_null_move_suppression);

        assert!(options.set_uci_option("Search Restriction Ordering Scale", "50"));
        assert_eq!(options.search.restriction_ordering_scale, 50);
    }

    #[test]
    fn eval_scales_are_clamped() {
        let mut options = EngineOptions::default();

        assert!(options.set_uci_option("Eval Mobility Scale", "999"));
        assert_eq!(options.eval.mobility_scale, 300);

        assert!(options.set_uci_option("Eval Mobility Scale", "-10"));
        assert_eq!(options.eval.mobility_scale, 0);
    }
}
