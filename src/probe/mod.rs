// ProbeBus — global diagnostic event bus.
//
// When the `probes` feature is enabled, every module can send typed events
// via the `probe!()` and `sample_probe!()` macros.  A dedicated writer thread
// serializes them as JSONL to `logs/boa-probe-<timestamp>.jsonl`.
//
// One file per search (per `go` command).  Call `open_search()` before the
// search and `close_search()` after.  The engine continues normally if the
// log file cannot be opened.
//
// Without `probes`, all macros expand to nothing — zero compile cost.

pub mod events;

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

static PROBE_BUS: RwLock<Option<Arc<ProbeBus>>> = RwLock::new(None);

#[cfg(feature = "probes")]
std::thread_local! {
    static SAMPLE_CTR: std::cell::Cell<u64> = std::cell::Cell::new(0);
}

/// Returns true every `rate` calls (per thread, deterministic).
#[cfg(feature = "probes")]
pub fn should_sample(rate: u32) -> bool {
    if rate <= 1 {
        return true;
    }
    SAMPLE_CTR.with(|c| {
        let v = c.get();
        c.set(v.wrapping_add(1));
        v % rate as u64 == 0
    })
}

const CHANNEL_CAP: usize = 8192;

pub struct ProbeBus {
    tx: SyncSender<Vec<u8>>,
    dropped: AtomicU64,
}

impl ProbeBus {
    /// Open a new probe file for the upcoming search.  Finishes any previous
    /// bus so the old file is flushed and closed.
    pub fn open_search(log_dir: &str) {
        let _ = std::fs::create_dir_all(log_dir);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = format!("{}/boa-probe-{}.jsonl", log_dir, ts);

        let file = match std::fs::File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("info string ProbeBus: cannot create {}: {}", path, e);
                return;
            }
        };
        let mut writer = std::io::BufWriter::new(file);

        // Write meta header
        let _ = writeln!(writer, "{}", Self::meta_json());
        let _ = writer.flush();

        let (tx, rx) = sync_channel::<Vec<u8>>(CHANNEL_CAP);

        std::thread::Builder::new()
            .name("probe-writer".into())
            .spawn(move || {
                let mut buf = writer;
                loop {
                    match rx.recv() {
                        Ok(data) => {
                            if data.is_empty() {
                                // Finish sentinel
                                let _ = buf.flush();
                                break;
                            }
                            let _ = buf.write_all(&data);
                            let _ = buf.write_all(b"\n");
                        }
                        Err(_) => break,
                    }
                }
                let _ = buf.flush();
            })
            .ok();

        let new_bus = Arc::new(ProbeBus {
            tx,
            dropped: AtomicU64::new(0),
        });

        // Rotate: finish old, install new
        let mut guard = PROBE_BUS.write().unwrap();
        if let Some(old) = guard.take() {
            old.finish();
        }
        *guard = Some(new_bus);
    }

    /// Close the current search's probe file.
    pub fn close_search() {
        if let Some(bus) = PROBE_BUS.write().unwrap().take() {
            bus.finish();
        }
    }

    /// Get a reference-counted handle to the current bus.  Returns `None`
    /// if probes are not initialized (e.g., feature disabled, or no search
    /// active between `close_search` and `open_search`).
    pub fn get() -> Option<Arc<ProbeBus>> {
        PROBE_BUS.read().unwrap().clone()
    }

    /// Non-blocking send.  Drops event if channel is full.
    pub fn send_json(&self, json: Vec<u8>) {
        match self.tx.try_send(json) {
            Ok(()) => {}
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {}
        }
    }

    /// Events dropped due to channel full.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Signal the writer thread to flush and exit.
    pub fn finish(&self) {
        let _ = self.tx.try_send(Vec::new());
    }

    fn meta_json() -> String {
        r#"{"typ":"meta","v":2,"fields":{"cf":{"ms":"tt_size_mb","ma":"material_scale","ps":"pst_scale","mo":"mobility_scale","ks":"king_safety_scale","pa":"pawn_structure_scale","co":"contempt","sy":"syzygy_enabled","md":"max_depth","mt":"move_time","wt":"wtime","bt":"btime","wi":"winc","bi":"binc","mg":"moves_to_go"},"b":{"f":"fen","p":"phase","nm":"non_pawn_material","mo":"mobile_pieces","of":"open_files","ck":"in_check","mr":"material_rule_score","hm":"halfmove_clock","fl":"fullmove_number"},"mg":{"nc":"total_count","qc":"quiet_count","cc":"capture_count","ec":"evasion_count","pc":"promotion_count","ck":"in_check"},"ev":{"ph":"phase","ma_mg":"material_mg","ma_eg":"material_eg","ma_cp":"material_cp","ps_mg":"pst_mg","ps_eg":"pst_eg","ps_cp":"pst_cp","mo_mg":"mobility_mg","mo_eg":"mobility_eg","mo_cp":"mobility_cp","mw":"mobility_white","mb":"mobility_black","pa_mg":"pawn_structure_mg","pa_eg":"pawn_structure_eg","pa_cp":"pawn_structure_cp","ks_mg":"king_safety_mg","ks_eg":"king_safety_eg","ks_cp":"king_safety_cp","ws":"white_score","ss":"side_to_move_score"},"sn":{"d":"depth","p":"ply","se":"static_eval","a":"alpha","b":"beta","pv":"is_pv","cu":"is_cut_node","ck":"in_check","im":"improving","ps":"prev_static_eval","sc":"score","nm":"moves_searched","bf":"beta_cutoffs_this_node","fc":"first_move_cutoff","tm":"node_time_us","tb":"tb_hit","tt":"tt_hit"},"ss":{"td":"depth_completed","ns":"total_nodes","qs":"qsearch_nodes","tm":"time_ms","np":"nodes_per_sec","bm":"best_move","bs":"best_score","sd":"sel_depth","tt_p":"tt_probes","tt_h":"tt_hits","tt_c":"tt_cutoffs","bc":"beta_cutoffs","fc":"first_move_cutoffs","nm_t":"null_move_tries","nm_c":"null_move_cutoffs","rp":"rfp_cutoffs","fp_a":"ffp_attempts","fp_p":"ffp_prunes","lm_a":"lmr_attempts","lm_r":"lmr_actual_reductions","lm_rs":"lmr_re_searches","se_w":"see_win_caps","se_e":"see_equal_caps","se_l":"see_loss_caps","se_s":"see_loss_searched","ii_t":"iid_triggers","ii_s":"iid_successes","tb_h":"tb_hits","dr":"dropped_probe_events"},"tt":{"op":"operation","h":"hit","et":"entry_type","ed":"entry_depth","es":"entry_score","ag":"entry_age","si":"slot_index","re":"replaced","rd":"replaced_depth"},"tc":{"d":"depth","et":"entry_type","ed":"entry_depth","df":"depth_sufficient","sc":"cutoff_score","a":"alpha","b":"beta"},"fp":{"d":"depth","mi":"move_index","hs":"history_score","mg":"computed_margin","rg":"required_gain","pr":"pruned","cu":"is_cut_node"},"rp":{"d":"depth","se":"static_eval","b":"beta","mg":"computed_margin","pr":"pruned"},"lm":{"d":"depth","p":"ply","mi":"move_index","ms":"moves_searched","hs":"history_score","br":"base_reduction","ar":"actual_reduction","nd":"new_depth","ip":"improving","ki":"is_killer","co":"is_counter","tm":"tt_move_agreement","gc":"gives_check","pi":"moving_piece","cu":"is_cut_node"},"nm":{"d":"depth","se":"static_eval","b":"beta","r":"reduction","sc":"null_move_score","pr":"pruned"},"se":{"vl":"see_value","cv":"captured_value","th":"threshold","pr":"pruned_by_see","sr":"searched_despite_bad_see","px":"pin_excluded"},"qs":{"p":"ply","sp":"stand_pat_score","a":"alpha","b":"beta","sc":"final_score","nc":"captures_searched","dp":"delta_pruned_count","se":"see_pruned_count","ck":"in_check","fc":"futility_cutoff"},"aw":{"d":"depth","dl":"initial_delta","lo":"window_low","hi":"window_high","fh":"fail_high","fl":"fail_low","ex":"expansion_count","rs":"research_score"},"ii":{"d":"depth","rd":"reduced_depth","tf":"tt_move_found_after_iid","sc":"iid_search_score"},"mo":{"p":"ply","mi":"move_index","ph":"phase_picked","bf":"butterfly_score","kh":"killer_score","ch":"counter_score","ca":"capture_history_score","mv":"mvv_lva_base","tt":"tt_move_bonus","pr":"promotion_bonus"},"ht":{"ev":"event_type","ci":"color_index","pi":"piece_index","mx":"max_value_before","mn":"min_value_before","th":"threshold"},"rt":{"d":"depth","bm":"best_move","bs":"best_score","pv":"pv_line","bc":"best_move_changed","pc":"previous_best_move","it":"iteration_time_ms","ns":"nodes_this_iteration","af":"aspiration_fails"},"ti":{"d":"depth","st":"stability","sf":"stability_factor","sd":"score_delta","sc":"score_factor","nb":"not_best_pct","nf":"node_factor","cf":"combined_factor","at":"adjusted_time","dc":"decision"},"tm":{"al":"allocated","ha":"hard_limit","op":"optimum_time","el":"elapsed","mt":"moves_to_go","mp":"move_overhead","rm":"remaining_clock","ic":"increment"},"tz":{"rs":"result","dm":"distance_to_mate","pc":"piece_count","dz":"dtz_value","wp":"wdl_probe_success"},"dd":{"ty":"draw_type","p":"ply","co":"contempt_applied","sc":"score_returned"},"md":{"p":"ply","oa":"original_alpha","na":"clamped_alpha","ob":"original_beta","nb":"clamped_beta","pr":"pruned"},"ch":{"tb":"table","hr":"hit_rate","as":"avg_score","mx":"max_abs","uf":"update_freq"},"cr":{"cv":"correction_value","re":"raw_eval","ce":"corrected_eval","df":"diff","pc":"pawn_corr","np":"nonpawn_corr","cc":"cont_corr","pl":"ply"},"pc":{"d":"depth","b":"beta","pb":"prob_beta","se":"static_eval","at":"attempts","ac":"accepted","ps":"prob_score","ns":"nodes_saved"},"sx":{"d":"depth","tt":"tt_score","sb":"singular_beta","ss":"singular_score","ext":"extension","mc":"multi_cut"},"te":{"d":"depth","lr":"lmr_reduction"},"re":{"d":"depth"}}}"#.to_string()
    }
}

// ---- Macros ----

/// Send a probe event (always — no sampling).
///
/// Usage:
///   probe!(Ffp, FfpEvent { depth, margin, required_gain, pruned });
///   probe!(Config, ConfigEvent { tt_size_mb: 128, ... });  // explicit naming also ok
///
/// `variant` = ProbeEvent enum variant, `struct` = event struct name.
/// Convention: variant `Ffp` wraps struct `FfpEvent` (both in `events` module).
///
/// Expands to nothing without the `probes` feature.
#[macro_export]
macro_rules! probe {
    // Explicit fields: `field: value, ...`
    ($variant:ident, $struct:ident { $($field:ident : $value:expr),* $(,)? }) => {
        #[cfg(feature = "probes")]
        if let Some(bus) = $crate::probe::ProbeBus::get() {
            let event = $crate::probe::events::ProbeEvent::$variant(
                $crate::probe::events::$struct { $($field: $value),* }
            );
            if let Ok(json) = serde_json::to_vec(&event) {
                bus.send_json(json);
            }
        }
    };
    // Shorthand fields: `field,` (same name as value)
    ($variant:ident, $struct:ident { $($field:ident),+ $(,)? }) => {
        #[cfg(feature = "probes")]
        if let Some(bus) = $crate::probe::ProbeBus::get() {
            let event = $crate::probe::events::ProbeEvent::$variant(
                $crate::probe::events::$struct { $($field),+ }
            );
            if let Ok(json) = serde_json::to_vec(&event) {
                bus.send_json(json);
            }
        }
    };
}

/// Send a probe event with sampling — fires 1 in `rate` calls.
///
/// Usage: `sample_probe!(8, Ffp, FfpEvent { ... });`
///
/// Thread-local counter. `rate=1` = always (same as `probe!`).
#[macro_export]
macro_rules! sample_probe {
    // Explicit fields
    ($rate:expr, $variant:ident, $struct:ident { $($field:ident : $value:expr),* $(,)? }) => {
        #[cfg(feature = "probes")]
        if $crate::probe::should_sample($rate) {
            $crate::probe!($variant, $struct { $($field: $value),* });
        }
    };
    // Shorthand fields
    ($rate:expr, $variant:ident, $struct:ident { $($field:ident),+ $(,)? }) => {
        #[cfg(feature = "probes")]
        if $crate::probe::should_sample($rate) {
            $crate::probe!($variant, $struct { $($field),+ });
        }
    };
}

/// Open a new probe file for the upcoming search.
/// Call at the start of each `go` command.
#[macro_export]
macro_rules! probe_open {
    ($dir:expr) => {
        #[cfg(feature = "probes")]
        $crate::probe::ProbeBus::open_search($dir);
    };
}

/// Close the current search's probe file.
/// Call after the search completes.
#[macro_export]
macro_rules! probe_close {
    () => {
        #[cfg(feature = "probes")]
        $crate::probe::ProbeBus::close_search();
    };
}
