#[derive(Default, Clone)]
#[allow(dead_code)]
pub struct SearchStats {
    pub nodes: u64,
    pub qnodes: u64,

    pub tt_probes: u64,
    pub tt_hits: u64,
    pub tt_cutoffs: u64,

    pub beta_cutoffs: u64,
    pub first_move_cutoffs: u64,

    pub null_move_tries: u64,
    pub null_move_cutoffs: u64,

    pub rfp_cutoffs: u64,

    pub ffp_attempts: u64,
    pub ffp_prunes: u64,

    pub lmr_attempts: u64,
    pub lmr_actual_reductions: u64,
    pub lmr_re_searches: u64,

    pub see_win_caps: u64,
    pub see_equal_caps: u64,
    pub see_loss_caps: u64,
    pub see_loss_searched: u64,

    pub iid_triggers: u64,
    pub iid_successes: u64,

    pub tb_hits: u64,

    // Continuation history diagnostics
    pub cont1_total_quiet_moves: u64,
    pub cont1_nonzero_moves: u64,
    pub cont1_score_sum: i64,
    pub cont1_update_count: u64,
    pub cont2_update_count: u64,
    pub cont4_update_count: u64,
    pub cont6_update_count: u64,

    // Correction history diagnostics
    pub corr_update_count: u64,
    pub corr_value_sum: i64,
    pub corr_sample_count: u64,
}

impl SearchStats {
    #[allow(dead_code)]
    pub fn report(&self) -> String {
        let total = self.nodes + self.qnodes;
        let q_pct = if total > 0 {
            self.qnodes as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        let tt_hit_pct = if self.tt_probes > 0 {
            self.tt_hits as f64 / self.tt_probes as f64 * 100.0
        } else {
            0.0
        };
        let first_cut_pct = if self.beta_cutoffs > 0 {
            self.first_move_cutoffs as f64 / self.beta_cutoffs as f64 * 100.0
        } else {
            0.0
        };
        let null_cut_pct = if self.null_move_tries > 0 {
            self.null_move_cutoffs as f64 / self.null_move_tries as f64 * 100.0
        } else {
            0.0
        };
        let lmr_actual_pct = if self.lmr_attempts > 0 {
            self.lmr_actual_reductions as f64 / self.lmr_attempts as f64 * 100.0
        } else {
            0.0
        };
        let lmr_re_pct = if self.lmr_actual_reductions > 0 {
            self.lmr_re_searches as f64 / self.lmr_actual_reductions as f64 * 100.0
        } else {
            0.0
        };
        let total_see = self.see_win_caps + self.see_equal_caps + self.see_loss_caps;
        let see_win_pct = if total_see > 0 {
            self.see_win_caps as f64 / total_see as f64 * 100.0
        } else {
            0.0
        };
        let see_eq_pct = if total_see > 0 {
            self.see_equal_caps as f64 / total_see as f64 * 100.0
        } else {
            0.0
        };
        let see_loss_pct = if total_see > 0 {
            self.see_loss_caps as f64 / total_see as f64 * 100.0
        } else {
            0.0
        };
        format!(
            "nodes {} qnodes {} ({:.1}%) tt_probes {} tt_hits {} ({:.1}%) tt_cuts {} \
             beta_cuts {} first_move_cuts {} ({:.1}%) \
             null_tries {} null_cuts {} ({:.1}%) \
             rfp_cuts {} ffp_try {} ffp_prune {} lmr_cand {} lmr_reduced {} ({:.1}%) lmr_re {} ({:.1}%) \
             see+ {} ({:.1}%) see= {} ({:.1}%) see- {} ({:.1}%) see-searched {} \
             iid {} iid_ok {} tb_hits {}",
            self.nodes,
            self.qnodes,
            q_pct,
            self.tt_probes,
            self.tt_hits,
            tt_hit_pct,
            self.tt_cutoffs,
            self.beta_cutoffs,
            self.first_move_cutoffs,
            first_cut_pct,
            self.null_move_tries,
            self.null_move_cutoffs,
            null_cut_pct,
            self.rfp_cutoffs,
            self.ffp_attempts,
            self.ffp_prunes,
            self.lmr_attempts,
            self.lmr_actual_reductions,
            lmr_actual_pct,
            self.lmr_re_searches,
            lmr_re_pct,
            self.see_win_caps,
            see_win_pct,
            self.see_equal_caps,
            see_eq_pct,
            self.see_loss_caps,
            see_loss_pct,
            self.see_loss_searched,
            self.iid_triggers,
            self.iid_successes,
            self.tb_hits,
        )
    }
}
