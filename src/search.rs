// ============================================================
// search.rs - Alpha-beta search with pragmatic pruning policy
//
// Core algorithm: PVS (Principal Variation Search) with iterative deepening.
//
// Search modifications:
//   1. PVS with iterative deepening, aspiration windows, and TT cutoffs
//   2. Null-move pruning, futility pruning, LMR, and quiescence search
//   3. SEE-guided capture ordering and optional losing-capture pruning
//   4. Lazy SMP root search when multiple threads are requested
// ============================================================

use crate::board::{Board, Zobrist};
use crate::config::EngineOptions;
use crate::criticality::{
    should_probe as should_probe_criticality, CriticalityLabelSource, CriticalityLogger,
    CriticalityRecord,
};
use crate::eval::{evaluate, EvalContext};
use crate::movegen::{gen_captures, gen_moves, AttackTables, MoveList};
use crate::syzygy::SyzygyTablebase;
use crate::tt::{score_from_tt, score_to_tt, Bound, TranspositionTable};
use crate::types::*;

// ---- Search tuning constants ----
// Sources: Stockfish (SF), Chess Programming Wiki (CPW), or marked [NEEDS TUNING].

/// Aspiration window: initial half-width in centipawns.
/// SF uses ~10-20 with gradual widening; 25 is a conservative starting point. [NEEDS TUNING]
const ASPIRATION_DELTA: i32 = 25;

/// Aspiration: use full window below this depth (no point aspirating at low depth).
/// Standard practice: SF uses 4-5.
const ASPIRATION_MIN_DEPTH: u32 = 4;

/// Reverse futility pruning margin per depth unit (centipawns).
/// SF uses ~67-73 (tuned via SPRT). 80 is slightly aggressive. [NEEDS TUNING]
const RFP_MARGIN_PER_DEPTH: i32 = 100;

/// RFP: maximum depth at which to apply.
/// SF applies up to depth ~7-8. 6 is conservative.
const RFP_MAX_DEPTH: i32 = 5;

/// Null-move pruning: minimum depth to attempt.
/// Standard: 3 (CPW, SF).
const NULL_MOVE_MIN_DEPTH: i32 = 3;

/// Null-move reduction: base + depth/4.
/// SF uses 4 + depth/6 post-tuning; 3 + depth/4 is a common simpler formula (CPW).
const NULL_MOVE_BASE_R: i32 = 3;
const NULL_MOVE_DEPTH_DIVISOR: i32 = 4;

/// Late move reductions: minimum moves searched before applying LMR.
/// Classic conservative LMR: reduce only late quiet moves.
const LMR_FULL_DEPTH_MOVES: usize = 4;

/// LMR: minimum depth to start reducing.
/// Standard: 3 (CPW).
const LMR_MIN_DEPTH: i32 = 3;

/// LMR: log-product divisor. Smaller values reduce more aggressively.
const LMR_LOG_DIVISOR: f64 = 2.5;

/// LMR: normalize quiet history into a small reduction adjustment.
const LMR_HISTORY_CLAMP: i32 = 8_192;
const LMR_HISTORY_NORMALIZER: i32 = 4_096;

/// LMR: extra reduction when the static eval is improving for side to move.
/// Disabled for the learned-criticality baseline; improving remains logged as a feature.
const LMR_IMPROVING_BONUS: i32 = 0;

/// LMR: whether to scale reductions by PV/cut-node type.
const LMR_NODE_TYPE_SCALING: bool = true;

/// Conservative learned-criticality protection for LMR quiets.
///
/// This is the raw full logistic model trained from the 200-game post-integration
/// shadow-probe dataset at analysis/criticality/2026-06-21_093900804.
/// We use it only as a ranker: moves at or above the validation P99 score get
/// one ply of reduction protection.  Do not use the calibrated probability for
/// continuous scaling unless calibration improves materially.
const CRITICALITY_P99_LOGIT: f64 = -2.689_557_247_165_626;
const CRITICALITY_INTERCEPT: f64 = -3.815_606_153_861_211_6;

/// Quiescence delta pruning margin (centipawns).
/// If stand_pat + capture_value + margin < alpha, skip. ~200 is standard (SF, CPW).
const DELTA_PRUNING_MARGIN: i32 = 200;

/// History table overflow threshold — scale down when any entry exceeds this.
/// Prevents history scores from dominating move ordering. [NEEDS TUNING]
const HISTORY_OVERFLOW_THRESHOLD: i32 = 500_000;

/// Time management: assumed remaining moves when movestogo is not specified.
/// 30 is a common default (CPW). Conservative engines use 25-40.
const DEFAULT_MOVES_TO_GO: i64 = 30;

/// Time management: minimum time allocation in milliseconds.
const MIN_MOVE_TIME_MS: i64 = 10;

/// Time management: hard limit multiplier and additive cap.
/// Prevents flagging by limiting total time to soft_budget * multiplier, capped.
const HARD_TIME_MULTIPLIER: u64 = 5;
const HARD_TIME_ADDITIVE_CAP: u64 = 2000;

/// Time management: reserve for GUI/process latency per move. Without this,
/// the engine budgets 100% of the clock and forfeits on time at fast TCs.
const MOVE_OVERHEAD_MS: i64 = 30;

/// Internal Iterative Deepening: minimum depth to apply IID.
/// When no TT move is available, do a reduced search to find a candidate.
/// Standard: 4-6 (CPW). Applied at PV nodes and high-depth non-PV nodes.
const IID_MIN_DEPTH: i32 = 5;

/// IID: depth reduction for the internal search.
/// Common formula: depth - 2 or depth - depth/4 - 1.
const IID_REDUCTION: i32 = 2;

/// Capture history: scale divisor when adding to MVV-LVA score.
/// Keeps learned capture ordering from overwhelming the static MVV-LVA signal.
const CAP_HISTORY_DIVISOR: i32 = 16;

/// Quiescence check evasion cap. In-check stand-pat is illegal, but unlimited
/// evasion recursion was too expensive; this keeps the tactical fix bounded.
const QS_CHECK_EVASION_MAX_PLY: usize = 2;

// ---- Search statistics (diagnostic) ----

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
             rfp_cuts {} lmr_cand {} lmr_reduced {} ({:.1}%) lmr_re {} ({:.1}%) \
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

// ---- Search limits ----

#[derive(Clone, Copy)]
pub struct Limits {
    pub max_depth: u32,
    pub nodes: u64,     // 0 = unlimited
    pub move_time: u64, // milliseconds, 0 = unlimited
    pub wtime: i64,
    pub btime: i64,
    pub winc: i64,
    pub binc: i64,
    pub moves_to_go: i32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_depth: 64,
            nodes: 0,
            move_time: 0,
            wtime: 0,
            btime: 0,
            winc: 0,
            binc: 0,
            moves_to_go: 0,
        }
    }
}

// ---- Search result ----

pub struct SearchResult {
    pub best_move: Move,
    pub score: Score,
    #[allow(dead_code)]
    pub depth: u32,
    pub nodes: u64,
    #[allow(dead_code)]
    pub pv: Vec<Move>,
}

#[derive(Clone, Copy)]
struct SearchNode {
    alpha: Score,
    beta: Score,
    depth: i32,
    ply: usize,
    is_pv: bool,
}

#[derive(Clone, Copy)]
struct MoveScoreContext {
    tt_move: Move,
    ply: usize,
    counter: Move,
    us: usize,
}

#[derive(Clone, Copy)]
struct LmrInput {
    moves_searched: usize,
    move_index: usize,
    ply: usize,
    depth: i32,
    history_score: i32,
    static_eval: Score,
    prev_static_eval: Option<Score>,
    alpha: Score,
    beta: Score,
    root_depth: i32,
    side_to_move: Color,
    moving_piece: Piece,
    is_pv: bool,
    is_cut_node: bool,
    improving: bool,
    is_killer: bool,
    is_counter: bool,
    tt_move_agreement: bool,
    is_capture: bool,
    is_promo: bool,
    gives_check: bool,
    in_check: bool,
}

#[derive(Clone, Copy)]
struct LmrReduction {
    base_reduction: i32,
    final_reduction: i32,
}

struct CriticalityRecordInput {
    enabled: bool,
    node_hash: u64,
    side_to_move: Color,
    m: Move,
    ply: usize,
    from: Square,
    to: Square,
    moving_piece: Piece,
    depth: i32,
    move_index: usize,
    base_reduction: i32,
    final_reduction: i32,
    new_depth: i32,
    history_score: i32,
    static_eval: Score,
    prev_static_eval: Option<Score>,
    alpha: Score,
    beta: Score,
    is_pv: bool,
    is_cut_node: bool,
    improving: bool,
    is_killer: bool,
    is_counter: bool,
    tt_move_agreement: bool,
}

// ---- Search context ----

pub struct SearchContext<'a> {
    pub atk: &'a AttackTables,
    pub z: &'a Zobrist,
    pub tt: &'a TranspositionTable,
    pub limits: Limits,
    pub start_ms: u64,
    pub contempt: i32,
    pub options: EngineOptions,
    pub syzygy: Option<&'a SyzygyTablebase>,
    pub root_color: Color,
    pub game_id: u64,
    pub search_id: u64,
    pub criticality_logger: Option<CriticalityLogger>,
    in_criticality_probe: bool,

    // Set by the UCI input thread when "stop"/"quit" arrives mid-search.
    // Without this the engine can emit a stale bestmove into the next game
    // (cutechess then scores it as an illegal move).
    pub stop_flag: &'a std::sync::atomic::AtomicBool,

    // Position history for repetition detection
    pub history_hashes: Vec<u64>,

    // Per-search stats
    pub nodes: u64,
    pub stopped: bool,
    pub root_depth: i32, // Current iteration depth (for check extension cap)
    smp_worker_id: usize,

    // Killer moves: [ply][slot]
    pub killers: [[Move; 2]; 128],

    // Quiet history heuristic: [color][piece_type][to]
    pub history: [[[i32; 64]; 6]; 2],

    // Counter-move heuristic: [from][to] -> best reply
    pub counter: [[Move; 64]; 64],

    // Capture history: [color][moving_piece_type][to_sq][captured_piece_type] -> i32
    pub cap_history: [[[[i32; 6]; 64]; 6]; 2],

    // Stack info per ply
    pub stack: [PlyInfo; 128],

    // Diagnostic stats
    pub stats: SearchStats,
}

#[derive(Clone, Copy, Default)]
pub struct PlyInfo {
    pub current_move: Move,
    pub static_eval: Option<Score>,
}

impl<'a> SearchContext<'a> {
    pub fn new(
        atk: &'a AttackTables,
        z: &'a Zobrist,
        tt: &'a TranspositionTable,
        limits: Limits,
        history_hashes: Vec<u64>,
        contempt: i32,
        options: EngineOptions,
        syzygy: Option<&'a SyzygyTablebase>,
        stop_flag: &'a std::sync::atomic::AtomicBool,
        game_id: u64,
        search_id: u64,
    ) -> Self {
        let criticality_logger = match CriticalityLogger::open(&options.criticality.log_dir) {
            Ok(logger) => logger,
            Err(err) => {
                eprintln!("info string CriticalityLogDir error: {err}");
                None
            }
        };
        SearchContext {
            atk,
            z,
            tt,
            limits,
            stop_flag,
            start_ms: now_ms(),
            root_depth: 0,
            smp_worker_id: 0,
            contempt,
            options,
            syzygy,
            root_color: Color::White, // set by search() before iterating
            game_id,
            search_id,
            criticality_logger,
            in_criticality_probe: false,
            history_hashes,
            nodes: 0,
            stopped: false,
            killers: [[MOVE_NONE; 2]; 128],
            history: [[[0i32; 64]; 6]; 2],
            counter: [[MOVE_NONE; 64]; 64],
            cap_history: [[[[0i32; 6]; 64]; 6]; 2],
            stack: [PlyInfo::default(); 128],
            stats: SearchStats::default(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        now_ms() - self.start_ms
    }

    fn should_stop(&mut self) -> bool {
        if self.stopped {
            return true;
        }
        if self.limits.nodes > 0 && self.nodes >= self.limits.nodes {
            self.stopped = true;
            return true;
        }
        if self.nodes & 4095 == 0 {
            if self.stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                self.stopped = true;
                return true;
            }
            if self.limits.move_time > 0 && self.elapsed_ms() >= self.limits.move_time {
                self.stopped = true;
                return true;
            }
        }
        false
    }

    /// Returns (soft budget, hard limit) in ms. The hard limit never exceeds
    /// the remaining clock minus MOVE_OVERHEAD_MS, so we cannot flag on a
    /// single move even when the soft formula is generous.
    fn time_for_move(&self, side: Color) -> (u64, u64) {
        let (time, inc) = if side == Color::White {
            (self.limits.wtime, self.limits.winc)
        } else {
            (self.limits.btime, self.limits.binc)
        };
        if time <= 0 {
            return (0, 0);
        }
        let usable = (time - MOVE_OVERHEAD_MS).max(MIN_MOVE_TIME_MS);
        let mtg = if self.limits.moves_to_go > 0 {
            self.limits.moves_to_go as i64
        } else {
            DEFAULT_MOVES_TO_GO
        };
        let soft = (usable / mtg + inc / 2).clamp(MIN_MOVE_TIME_MS, usable) as u64;
        let hard = (soft * HARD_TIME_MULTIPLIER as i64 as u64)
            .min(soft + HARD_TIME_ADDITIVE_CAP)
            .min(usable as u64)
            .max(MIN_MOVE_TIME_MS as u64);
        (soft, hard)
    }

    /// Check if the current board position has appeared before (twofold repetition).
    /// Only the last `halfmove` entries can repeat — anything older is separated
    /// by an irreversible move (capture/pawn push) — and `any` short-circuits,
    /// so this is O(halfmove) instead of O(game length) per node.
    fn is_repetition(&self, board: &Board) -> bool {
        let lookback = (board.halfmove as usize).min(self.history_hashes.len());
        self.history_hashes
            .iter()
            .rev()
            .take(lookback)
            .any(|&h| h == board.hash)
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ============================================================
// Section 0: Bench
// ============================================================

const BENCH_FENS: [&str; 20] = [
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "r1bq1rk1/pp2ppbp/2n3p1/2pp4/5P2/2NP1NP1/PPP1P1BP/R1BQ1RK1 w - - 0 8",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    "r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
    "r1bqk2r/ppppbppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQ1RK1 w kq - 6 5",
    "r1bq1rk1/ppp2ppp/2np1n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 w - - 0 7",
    "r2qkbnr/ppp2ppp/2n1b3/3pp3/4P3/2N2N2/PPPP1PPP/R1BQKB1R w KQkq - 4 4",
    "rnbqk2r/ppp1bppp/4pn2/3p4/2PP4/2N2N2/PP2PPPP/R1BQKB1R w KQkq - 2 4",
    "r1bqkb1r/pp3ppp/2n1pn2/2pp4/2PP4/2N2N2/PP2PPPP/R1BQKB1R w KQkq - 0 5",
    "r1bqk2r/pp2bppp/2n1pn2/3p4/2PP4/2N2N2/PP2PPPP/R1BQ1RK1 b kq - 5 6",
    "rnbq1rk1/ppp1bppp/4pn2/3p4/2PP4/5NP1/PP2PPBP/RNBQK2R w KQ - 4 5",
    "r1bq1rk1/pppn1ppp/4pn2/3p4/1bPP4/2N2NP1/PP2PPBP/R1BQK2R w KQ - 2 6",
    "2r3k1/pp2ppbp/6p1/8/2Bn4/2N3P1/PP2PP1P/3R2K1 w - - 0 1",
    "r4rk1/pp2ppbp/2np2p1/q7/4P3/2N1BP2/PPPQ2PP/R4RK1 w - - 0 1",
    "r2q1rk1/pb1nbppp/1pp1pn2/3p4/2PP4/1PN1PN2/PB2BPPP/R2Q1RK1 w - - 0 9",
    "r1b2rk1/2q1bppp/p2ppn2/1p6/3QP3/1BN1BP2/PPP3PP/R4RK1 w - - 0 12",
    "2rr2k1/1p3ppp/p1nbbp2/4p3/4P3/1NN1B1P1/PP3PBP/R4RK1 w - - 0 1",
];

pub fn bench(atk: &AttackTables, z: &Zobrist, depth: u32) {
    use crate::tt::TranspositionTable;
    let mut tt = TranspositionTable::new(64);
    let mut total_nodes = 0u64;
    let start = now_ms();

    let no_stop = std::sync::atomic::AtomicBool::new(false);
    for (i, fen) in BENCH_FENS.iter().enumerate() {
        let mut board = Board::from_fen(fen).unwrap();
        let limits = Limits {
            max_depth: depth,
            move_time: 10_000,
            ..Limits::default()
        };
        let mut ctx = SearchContext::new(
            atk,
            z,
            &mut tt,
            limits,
            Vec::new(),
            20,
            EngineOptions::default(),
            None,
            &no_stop,
            0,
            i as u64 + 1,
        );
        let result = search(&mut board, &mut ctx);
        total_nodes += result.nodes;
        eprintln!(
            "bench {}/{}: {} nodes  score {} pv {}",
            i + 1,
            BENCH_FENS.len(),
            result.nodes,
            result.score,
            move_name(result.best_move)
        );
        tt.clear();
    }

    let elapsed = (now_ms() - start).max(1);
    let nps = total_nodes * 1000 / elapsed;
    eprintln!("============================");
    eprintln!("Total time  : {} ms", elapsed);
    eprintln!("Total nodes : {}", total_nodes);
    eprintln!("Nodes/sec   : {}", nps);
    eprintln!("Positions   : {}", BENCH_FENS.len());
    eprintln!("Depth       : {}", depth);
    println!("{}", total_nodes);
}

// ============================================================
// Section 1: Iterative deepening
// ============================================================

pub fn search(board: &mut Board, ctx: &mut SearchContext) -> SearchResult {
    let threads = ctx.options.search.threads.clamp(1, 64);
    if ctx.options.search.lazy_smp && threads > 1 && ctx.limits.nodes == 0 {
        return lazy_smp_search(board, ctx, threads);
    }
    search_single(board, ctx, true, true)
}

fn lazy_smp_search(board: &mut Board, ctx: &mut SearchContext, threads: usize) -> SearchResult {
    ctx.tt.new_search();

    let atk = ctx.atk;
    let z = ctx.z;
    let tt = ctx.tt;
    let limits = ctx.limits;
    let history = ctx.history_hashes.clone();
    let contempt = ctx.contempt;
    let syzygy = ctx.syzygy;
    let stop_flag = ctx.stop_flag;
    let game_id = ctx.game_id;
    let search_id = ctx.search_id;
    let mut worker_options = ctx.options.clone();
    worker_options.search.threads = 1;
    worker_options.criticality.log_dir.clear();
    worker_options.criticality.probe_permille = 0;

    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(threads.saturating_sub(1));
        for worker_id in 1..threads {
            let mut worker_board = board.clone();
            let worker_history = history.clone();
            let worker_options = worker_options.clone();
            handles.push(scope.spawn(move || {
                let mut worker_ctx = SearchContext::new(
                    atk,
                    z,
                    tt,
                    limits,
                    worker_history,
                    contempt,
                    worker_options,
                    syzygy,
                    stop_flag,
                    game_id,
                    search_id,
                );
                worker_ctx.smp_worker_id = worker_id;
                search_single(&mut worker_board, &mut worker_ctx, false, false)
            }));
        }

        let mut result = search_single(board, ctx, true, false);
        stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);

        for handle in handles {
            if let Ok(worker_result) = handle.join() {
                result.nodes += worker_result.nodes;
            }
        }
        result
    })
}

fn search_single(
    board: &mut Board,
    ctx: &mut SearchContext,
    emit_info: bool,
    advance_tt_age: bool,
) -> SearchResult {
    if advance_tt_age {
        ctx.tt.new_search();
    }
    ctx.nodes = 0;
    ctx.stopped = false;
    ctx.stats = SearchStats::default();
    ctx.root_color = board.side;

    let (time_budget, hard_budget) = ctx.time_for_move(board.side);
    let hard_limit = if ctx.limits.move_time > 0 {
        ctx.limits.move_time
    } else {
        hard_budget
    };
    if hard_limit > 0 {
        ctx.limits.move_time = hard_limit;
    }

    let mut best_move = MOVE_NONE;
    let mut best_score = -SCORE_INF;
    let mut pv = Vec::new();
    let mut completed_depth = 0;

    if let Some(tb) = ctx.syzygy {
        if let Some(root_probe) = tb.probe_root(board, ctx.atk, ctx.z, &ctx.options.syzygy) {
            ctx.stats.tb_hits += 1;
            if emit_info {
                println!(
                    "info depth 0 score cp {} nodes {} time {} tbhits {} string syzygy wdl {} dtz {}",
                    root_probe.score,
                    ctx.nodes,
                    ctx.elapsed_ms(),
                    ctx.stats.tb_hits,
                    root_probe.wdl,
                    root_probe.dtz
                );
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            return SearchResult {
                best_move: root_probe.best_move,
                score: root_probe.score,
                depth: 0,
                nodes: ctx.nodes,
                pv: vec![root_probe.best_move],
            };
        }
    }

    for depth in 1..=ctx.limits.max_depth {
        ctx.root_depth = depth as i32;
        let mut root_pv = Vec::new();
        let score = aspiration_search(board, ctx, depth, best_score, &mut root_pv);

        if ctx.stopped {
            break;
        }

        best_score = score;
        if !root_pv.is_empty() {
            best_move = root_pv[0];
            pv = root_pv;
        }

        // Report to UCI
        let elapsed = ctx.elapsed_ms().max(1);
        let nps = ctx.nodes * 1000 / elapsed;
        let score_str = if is_mate_score(score) {
            format!("mate {}", mate_in(score))
        } else {
            format!("cp {}", score)
        };
        let pv_str: String = pv.iter().map(|&m| move_name(m) + " ").collect();
        if emit_info {
            println!(
                "info depth {} score {} nodes {} nps {} time {} hashfull {} pv {}",
                depth,
                score_str,
                ctx.nodes,
                nps,
                elapsed,
                ctx.tt.hashfull(),
                pv_str.trim()
            );
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        completed_depth = depth;

        // Time management: stop if we've used our soft budget
        if time_budget > 0 && ctx.elapsed_ms() >= time_budget {
            break;
        }
        if is_mate_score(score) {
            break;
        }
    }

    // Never return MOVE_NONE: if the search was stopped before even depth 1
    // completed (deep time trouble), "bestmove 0000" forfeits the game as an
    // illegal move. Fall back to the first legal move.
    if best_move == MOVE_NONE {
        let list = gen_moves(board, ctx.atk);
        for i in 0..list.count {
            let m = list.moves[i];
            let undo = board.make_move(m, ctx.z);
            let legal = !board.is_in_check(board.side.flip());
            board.unmake_move(m, &undo, ctx.z);
            if legal {
                best_move = m;
                break;
            }
        }
    }

    if let Some(logger) = &mut ctx.criticality_logger {
        if let Err(err) = logger.flush() {
            eprintln!("info string criticality log flush failed: {err}");
        }
    }

    SearchResult {
        best_move,
        score: best_score,
        depth: completed_depth,
        nodes: ctx.nodes,
        pv,
    }
}

fn aspiration_search(
    board: &mut Board,
    ctx: &mut SearchContext,
    depth: u32,
    prev_score: Score,
    pv: &mut Vec<Move>,
) -> Score {
    if depth <= ASPIRATION_MIN_DEPTH {
        return alpha_beta(
            board,
            ctx,
            SearchNode {
                alpha: -SCORE_INF,
                beta: SCORE_INF,
                depth: depth as i32,
                ply: 0,
                is_pv: true,
            },
            pv,
        );
    }
    let delta = ASPIRATION_DELTA;
    let mut alpha = (prev_score - delta).max(-SCORE_INF);
    let mut beta = (prev_score + delta).min(SCORE_INF);
    let mut window_expand = 0;

    loop {
        let score = alpha_beta(
            board,
            ctx,
            SearchNode {
                alpha,
                beta,
                depth: depth as i32,
                ply: 0,
                is_pv: true,
            },
            pv,
        );
        if ctx.stopped {
            return score;
        }
        if score <= alpha {
            beta = (alpha + beta) / 2;
            alpha = (alpha - delta * (1 << window_expand)).max(-SCORE_INF);
            window_expand += 1;
        } else if score >= beta {
            beta = (beta + delta * (1 << window_expand)).min(SCORE_INF);
            window_expand += 1;
        } else {
            return score;
        }
        if alpha <= -SCORE_INF && beta >= SCORE_INF {
            break;
        }
    }
    alpha_beta(
        board,
        ctx,
        SearchNode {
            alpha: -SCORE_INF,
            beta: SCORE_INF,
            depth: depth as i32,
            ply: 0,
            is_pv: true,
        },
        pv,
    )
}

/// Try TT cutoff for non-PV nodes. Returns Some(score) if we can cut off.
fn try_tt_cutoff(
    ctx: &mut SearchContext,
    hash: u64,
    depth: i32,
    alpha: Score,
    beta: Score,
    is_pv: bool,
    ply: usize,
) -> (Move, Option<Score>) {
    ctx.stats.tt_probes += 1;
    let entry = match ctx.tt.probe(hash) {
        Some(e) => e,
        None => return (MOVE_NONE, None),
    };
    ctx.stats.tt_hits += 1;
    let tt_move = entry.best;

    if is_pv || entry.depth < depth as i8 {
        return (tt_move, None);
    }

    let s = score_from_tt(entry.score, ply);
    let cutoff = match entry.bound {
        Bound::Exact => true,
        Bound::Lower => s >= beta,
        Bound::Upper => s <= alpha,
        _ => false,
    };
    if cutoff {
        ctx.stats.tt_cutoffs += 1;
        return (tt_move, Some(s));
    }
    (tt_move, None)
}

/// Try null-move pruning. Returns Some(score) if we can cut off.
fn try_null_move(
    board: &mut Board,
    ctx: &mut SearchContext,
    beta: Score,
    depth: i32,
    ply: usize,
    static_eval: Score,
) -> Option<Score> {
    if depth < NULL_MOVE_MIN_DEPTH || static_eval < beta {
        return None;
    }
    let our_pieces = board.occ[board.side as usize]
        & !board.pieces[board.side as usize][PieceType::Pawn as usize]
        & !board.pieces[board.side as usize][PieceType::King as usize];
    if our_pieces == 0 {
        return None;
    }

    ctx.stats.null_move_tries += 1;
    let r = NULL_MOVE_BASE_R + depth / NULL_MOVE_DEPTH_DIVISOR;
    let undo = board.make_null_move(ctx.z);
    let mut null_pv = Vec::new();
    let null_score = -alpha_beta(
        board,
        ctx,
        SearchNode {
            alpha: -beta,
            beta: -beta + 1,
            depth: depth - r,
            ply: ply + 1,
            is_pv: false,
        },
        &mut null_pv,
    );
    board.unmake_null_move(&undo);

    if null_score >= beta {
        ctx.stats.null_move_cutoffs += 1;
        return Some(beta);
    }
    None
}

/// Handle beta cutoff: update killers, history, counter moves.
fn handle_beta_cutoff(
    ctx: &mut SearchContext,
    board: &Board,
    m: Move,
    ply: usize,
    depth: i32,
    is_capture: bool,
) {
    // Counterfactual probes are shadow-only: they may observe a full-depth
    // score, but must not train move-ordering heuristics used by the real search.
    if ctx.in_criticality_probe {
        return;
    }
    if is_capture {
        update_cap_history(ctx, board.side, m, board, depth);
        return;
    }
    update_killers(ctx, ply, m);
    let bonus = history_delta(depth);
    let moving_piece = board.sq_piece[move_from(m) as usize];
    add_history_score(ctx, board.side, moving_piece, m, bonus);
    if ply == 0 || ply >= 128 {
        return;
    }
    let prev_move = ctx.stack[ply - 1].current_move;
    if prev_move != MOVE_NONE {
        ctx.counter[move_from(prev_move) as usize][move_to(prev_move) as usize] = m;
    }
}

/// Score a single move for move ordering.
fn score_single_move(
    board: &mut Board,
    ctx: &SearchContext,
    m: Move,
    scoring: MoveScoreContext,
) -> i32 {
    if m == scoring.tt_move {
        return 2_000_000;
    }
    if move_flags(m) == MF_PROMOTION {
        return 1_800_000 + move_promo_pt(m).material_value();
    }

    let cap = board.sq_piece[move_to(m) as usize];
    let is_capture = cap != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
    if is_capture {
        let cap_val = if cap != PIECE_NONE {
            piece_type(cap).material_value()
        } else {
            100
        };
        let mov_val = piece_type(board.sq_piece[move_from(m) as usize]).material_value();
        let mover_pt = piece_type(board.sq_piece[move_from(m) as usize]) as usize;
        let cap_pt = if cap != PIECE_NONE {
            piece_type(cap) as usize
        } else {
            0
        };
        let to = move_to(m) as usize;
        let ch = ctx.cap_history[scoring.us][mover_pt][to][cap_pt] / CAP_HISTORY_DIVISOR;
        return 1_000_000 + cap_val * 10 - mov_val + ch;
    }

    // Quiet move scoring
    let mut s = 0i32;
    if scoring.ply < 128 && m == ctx.killers[scoring.ply][0] {
        s += 900_000;
    } else if scoring.ply < 128 && m == ctx.killers[scoring.ply][1] {
        s += 800_000;
    } else if m == scoring.counter {
        s += 750_000;
    }
    let mover = board.sq_piece[move_from(m) as usize];
    if mover != PIECE_NONE {
        s += ctx.history[scoring.us][piece_type(mover) as usize][move_to(m) as usize];
    }
    s
}

// ============================================================
// Section 2: Alpha-beta (PVS)
// ============================================================

fn alpha_beta(
    board: &mut Board,
    ctx: &mut SearchContext,
    node: SearchNode,
    pv: &mut Vec<Move>,
) -> Score {
    let SearchNode {
        alpha,
        beta,
        depth,
        ply,
        is_pv,
    } = node;

    if ctx.should_stop() {
        return 0;
    }
    ctx.nodes += 1;
    ctx.stats.nodes += 1;

    // Draw detection: repetition, 50-move, insufficient material
    if ply > 0 {
        if board.halfmove >= 100 || is_insufficient_material(board) {
            // Contempt: positive from root side's view (root side avoids draws)
            let sign = if board.side == ctx.root_color { 1 } else { -1 };
            return SCORE_DRAW - ctx.contempt * sign;
        }
        // Repetition detection — check against ancestors (not self)
        if ctx.is_repetition(board) {
            let sign = if board.side == ctx.root_color { 1 } else { -1 };
            return SCORE_DRAW - ctx.contempt * sign;
        }
    }

    if let Some(tb) = ctx.syzygy {
        if let Some(score) = tb.probe_score(board, &ctx.options.syzygy, depth, ply) {
            ctx.stats.tb_hits += 1;
            return score;
        }
    }

    // Push current position hash so children/grandchildren can detect repetition
    ctx.history_hashes.push(board.hash);

    // Mate distance pruning
    let mut alpha = alpha.max(-(SCORE_MATE - ply as Score));
    let beta_md = beta.min(SCORE_MATE - ply as Score - 1);
    if alpha >= beta_md {
        ctx.history_hashes.pop();
        return alpha;
    }
    let beta = beta_md;
    let is_cut_node = !is_pv && beta == alpha + 1;

    let in_check = board.is_in_check(board.side);

    // A side in check cannot legally stand pat. If depth is exhausted while in
    // check, continue through the normal move loop so checkmates and evasions
    // are scored by legal play instead of by static evaluation.
    let depth = if depth <= 0 && in_check { 1 } else { depth };

    // Drop into quiescence at depth 0 only for quiet-to-move positions.
    if depth <= 0 {
        ctx.history_hashes.pop();
        return quiescence(board, ctx, alpha, beta, ply, 0);
    }

    // Check extension: extend by 1 ply when in check.
    // Absolute ply cap based on current iteration depth, not max_depth.
    // This prevents search explosion in endgames with long checking sequences.
    let ply_limit = ctx.root_depth as usize + 2;
    let depth = if in_check && depth >= 4 && ply < ply_limit {
        depth + 1
    } else {
        depth
    };

    // TT probe
    let (mut tt_move, tt_cutoff) = try_tt_cutoff(ctx, board.hash, depth, alpha, beta, is_pv, ply);
    if let Some(s) = tt_cutoff {
        ctx.history_hashes.pop();
        return s;
    }

    // ---- Internal Iterative Deepening (IID) ----
    // When we have no TT move at a PV node (or high-depth non-PV), do a
    // reduced-depth search to populate the TT with a candidate best move.
    if tt_move == MOVE_NONE && depth >= IID_MIN_DEPTH && !in_check {
        ctx.stats.iid_triggers += 1;
        let iid_depth = depth - IID_REDUCTION;
        let mut iid_pv = Vec::new();
        // Temporarily remove our hash to prevent false repetition detection
        ctx.history_hashes.pop();
        alpha_beta(
            board,
            ctx,
            SearchNode {
                alpha,
                beta,
                depth: iid_depth,
                ply,
                is_pv,
            },
            &mut iid_pv,
        );
        ctx.history_hashes.push(board.hash);
        // Re-probe TT — the reduced search will have stored a best move
        if let Some(entry) = ctx.tt.probe(board.hash) {
            tt_move = entry.best;
            ctx.stats.iid_successes += 1;
        }
    }

    // Static evaluation for pruning heuristics
    let static_eval = evaluate(
        board,
        &EvalContext {
            atk: ctx.atk,
            options: &ctx.options,
        },
    );
    let improving = is_improving(ctx, static_eval, ply);
    if ply < MAX_PLY {
        ctx.stack[ply].static_eval = Some(static_eval);
    }
    // ---- Pruning heuristics (skip in check and PV nodes) ----

    if !in_check && !is_pv {
        // Reverse futility pruning (static null move)
        let rfp_margin = RFP_MARGIN_PER_DEPTH * depth;
        if depth <= RFP_MAX_DEPTH && static_eval - rfp_margin >= beta && !is_mate_score(static_eval)
        {
            ctx.stats.rfp_cutoffs += 1;
            ctx.history_hashes.pop();
            return static_eval - rfp_margin;
        }

        if let Some(null_score) = try_null_move(board, ctx, beta, depth, ply, static_eval) {
            ctx.history_hashes.pop();
            return null_score;
        }
    }

    // Generate and order moves
    let mut list = gen_moves(board, ctx.atk);
    score_moves(board, ctx, &mut list, tt_move, ply);
    if ply == 0 && ctx.smp_worker_id > 0 && list.count > 1 {
        let idx = ctx.smp_worker_id % list.count;
        list.scores[idx] += 3_000_000;
    }

    let mut best_move = MOVE_NONE;
    let mut best_score = -SCORE_INF;
    let mut bound = Bound::Upper;
    let mut moves_searched = 0;
    let mut quiet_moves_searched = 0;
    let mut legal_moves = 0;

    for i in 0..list.count {
        list.pick_best(i);
        let m = list.moves[i];
        let node_hash = board.hash;
        let side_to_move = board.side;
        let from = move_from(m);
        let to = move_to(m);
        let moving_piece = board.sq_piece[from as usize];
        let prev_static_eval = if ply >= 2 && ply - 2 < MAX_PLY {
            ctx.stack[ply - 2].static_eval
        } else {
            None
        };
        let prev_move = if ply > 0 && ply < MAX_PLY {
            ctx.stack[ply - 1].current_move
        } else {
            MOVE_NONE
        };
        let counter_move = if prev_move != MOVE_NONE {
            ctx.counter[move_from(prev_move) as usize][move_to(prev_move) as usize]
        } else {
            MOVE_NONE
        };

        // Make move and verify legality
        let undo = board.make_move(m, ctx.z);
        if board.is_in_check(board.side.flip()) {
            board.unmake_move(m, &undo, ctx.z);
            continue;
        }
        legal_moves += 1;

        let is_capture = undo.captured != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
        let is_promo = move_flags(m) == MF_PROMOTION;
        let gives_check = board.is_in_check(board.side);
        let mover = board.side.flip() as usize;
        let history_score = if moving_piece != PIECE_NONE {
            ctx.history[mover][piece_type(moving_piece) as usize][move_to(m) as usize]
        } else {
            0
        };
        let is_killer = ply < 128 && (m == ctx.killers[ply][0] || m == ctx.killers[ply][1]);
        let is_counter = m == counter_move;
        let is_lmr_quiet = !is_capture && !is_promo && !gives_check;

        // ---- Late move reductions (LMR) ----
        let lmr = compute_lmr_reduction_details(
            LmrInput {
                moves_searched: quiet_moves_searched,
                move_index: moves_searched + 1,
                ply,
                depth,
                history_score,
                static_eval,
                prev_static_eval,
                alpha,
                beta,
                root_depth: ctx.root_depth,
                side_to_move,
                moving_piece,
                is_pv,
                is_cut_node,
                improving,
                is_killer,
                is_counter,
                tt_move_agreement: m == tt_move,
                is_capture,
                is_promo,
                gives_check,
                in_check,
            },
            ctx,
        );
        let reduction = lmr.final_reduction;
        if reduction > 0 {
            ctx.stats.lmr_actual_reductions += 1;
        }

        let new_depth = if reduction > 0 {
            (depth - 1 - reduction).max(1)
        } else {
            depth - 1
        };
        let pre_alpha = alpha;
        let pre_beta = beta;
        let mut criticality_record = build_criticality_record(
            ctx,
            CriticalityRecordInput {
                enabled: reduction > 0 && !ctx.in_criticality_probe,
                node_hash,
                side_to_move,
                m,
                ply,
                from,
                to,
                moving_piece,
                depth,
                move_index: moves_searched + 1,
                base_reduction: lmr.base_reduction,
                final_reduction: reduction,
                new_depth,
                history_score,
                static_eval,
                prev_static_eval,
                alpha: pre_alpha,
                beta: pre_beta,
                is_pv,
                is_cut_node,
                improving,
                is_killer,
                is_counter,
                tt_move_agreement: m == tt_move,
            },
        );

        // Record the move being searched BEFORE recursing — children read
        // stack[ply].current_move for the counter-move heuristic. (Previously
        // set after the search returned, so children always saw the previous
        // sibling's move and the counter table learned garbage.)
        if ply < 128 {
            ctx.stack[ply].current_move = m;
        }

        let mut child_pv = Vec::new();
        let score = if moves_searched == 0 {
            -alpha_beta(
                board,
                ctx,
                SearchNode {
                    alpha: -beta,
                    beta: -alpha,
                    depth: depth - 1,
                    ply: ply + 1,
                    is_pv,
                },
                &mut child_pv,
            )
        } else {
            let mut s = -alpha_beta(
                board,
                ctx,
                SearchNode {
                    alpha: -alpha - 1,
                    beta: -alpha,
                    depth: new_depth,
                    ply: ply + 1,
                    is_pv: false,
                },
                &mut child_pv,
            );
            if let Some(record) = &mut criticality_record {
                record.reduced_score = Some(s);
            }
            if !ctx.stopped && s > alpha && (s < beta || reduction > 0) {
                if reduction > 0 {
                    ctx.stats.lmr_re_searches += 1;
                }
                child_pv.clear();
                s = -alpha_beta(
                    board,
                    ctx,
                    SearchNode {
                        alpha: -beta,
                        beta: -alpha,
                        depth: depth - 1,
                        ply: ply + 1,
                        is_pv,
                    },
                    &mut child_pv,
                );
                if let Some(record) = &mut criticality_record {
                    record.label_source = CriticalityLabelSource::ObservedResearch;
                    record.full_score = Some(s);
                }
            } else if should_run_criticality_probe(
                ctx, node_hash, m, depth, ply, reduction, s, pre_alpha,
            ) {
                // Shadow-only counterfactual: record the full-depth score, but
                // keep the reduced score/PV as the actual search result.
                let reduced_child_pv = child_pv.clone();
                child_pv.clear();
                let was_in_probe = ctx.in_criticality_probe;
                ctx.in_criticality_probe = true;
                let full_score = -alpha_beta(
                    board,
                    ctx,
                    SearchNode {
                        alpha: -beta,
                        beta: -alpha,
                        depth: depth - 1,
                        ply: ply + 1,
                        is_pv,
                    },
                    &mut child_pv,
                );
                ctx.in_criticality_probe = was_in_probe;
                child_pv = reduced_child_pv;
                if !ctx.stopped {
                    if let Some(record) = &mut criticality_record {
                        record.label_source = CriticalityLabelSource::CounterfactualProbe;
                        record.full_score = Some(full_score);
                    }
                }
            }
            s
        };

        if !ctx.stopped {
            if let Some(record) = &criticality_record {
                write_criticality_record(ctx, record);
            }
        }

        board.unmake_move(m, &undo, ctx.z);
        moves_searched += 1;
        if is_lmr_quiet {
            quiet_moves_searched += 1;
        }

        if ctx.stopped {
            ctx.history_hashes.pop();
            return 0;
        }

        if !ctx.in_criticality_probe && is_lmr_quiet && score <= pre_alpha {
            add_history_score(ctx, side_to_move, moving_piece, m, -history_delta(depth));
        }

        if score > best_score {
            best_score = score;
            best_move = m;
            if score > alpha {
                alpha = score;
                bound = Bound::Exact;
                pv.clear();
                pv.push(m);
                pv.extend_from_slice(&child_pv);
            }
        }

        if score >= beta {
            ctx.stats.beta_cutoffs += 1;
            if moves_searched == 1 {
                ctx.stats.first_move_cutoffs += 1;
            }
            bound = Bound::Lower;
            handle_beta_cutoff(ctx, board, m, ply, depth, is_capture);
            break;
        }
    }

    // Checkmate or stalemate
    if legal_moves == 0 {
        ctx.history_hashes.pop();
        let sign = if board.side == ctx.root_color { 1 } else { -1 };
        return if in_check {
            -(SCORE_MATE - ply as Score)
        } else {
            SCORE_DRAW - ctx.contempt * sign
        };
    }

    // Pop our position hash from history
    ctx.history_hashes.pop();

    // TT store (mate scores converted to node-relative distance). Do not let
    // shadow probes seed the TT for the real search after they return.
    if !ctx.in_criticality_probe {
        ctx.tt.store(
            board.hash,
            score_to_tt(best_score, ply),
            best_move,
            depth as i8,
            bound,
        );
    }

    best_score
}

fn quiescence(
    board: &mut Board,
    ctx: &mut SearchContext,
    mut alpha: Score,
    beta: Score,
    ply: usize,
    qs_ply: usize,
) -> Score {
    if ctx.should_stop() {
        return 0;
    }
    ctx.nodes += 1;
    ctx.stats.qnodes += 1;

    if board.is_in_check(board.side) {
        if qs_ply >= QS_CHECK_EVASION_MAX_PLY || ply >= MAX_PLY {
            return evaluate(
                board,
                &EvalContext {
                    atk: ctx.atk,
                    options: &ctx.options,
                },
            );
        }

        let mut list = gen_moves(board, ctx.atk);
        score_moves(board, ctx, &mut list, MOVE_NONE, ply);

        let mut legal_moves = 0;
        for i in 0..list.count {
            list.pick_best(i);
            let m = list.moves[i];

            let undo = board.make_move(m, ctx.z);
            if board.is_in_check(board.side.flip()) {
                board.unmake_move(m, &undo, ctx.z);
                continue;
            }
            legal_moves += 1;

            if ply < 128 {
                ctx.stack[ply].current_move = m;
            }
            let score = -quiescence(board, ctx, -beta, -alpha, ply + 1, qs_ply + 1);
            board.unmake_move(m, &undo, ctx.z);

            if ctx.stopped {
                return 0;
            }
            if score >= beta {
                return score;
            }
            if score > alpha {
                alpha = score;
            }
        }

        if legal_moves == 0 {
            return -(SCORE_MATE - ply as Score);
        }
        return alpha;
    }

    // Normal quiescence: captures only. In-check nodes are handled above with
    // a small evasion cap, because standing pat while in check is illegal.
    let stand_pat = evaluate(
        board,
        &EvalContext {
            atk: ctx.atk,
            options: &ctx.options,
        },
    );
    if stand_pat >= beta {
        return stand_pat;
    }
    if stand_pat > alpha {
        alpha = stand_pat;
    }

    let mut list = gen_captures(board, ctx.atk);

    score_captures(board, ctx, &mut list);

    for i in 0..list.count {
        list.pick_best(i);
        let m = list.moves[i];

        // Delta pruning (only for captures, not for checks)
        let cap_piece = board.sq_piece[move_to(m) as usize];
        let is_capture = cap_piece != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
        let cap_value = if cap_piece != PIECE_NONE {
            piece_type(cap_piece).material_value()
        } else if move_flags(m) == MF_EN_PASSANT {
            PieceType::Pawn.material_value()
        } else {
            0
        };
        if is_capture && stand_pat + cap_value + DELTA_PRUNING_MARGIN < alpha {
            continue;
        }

        if ctx.options.search.see && ctx.options.search.see_qsearch_pruning {
            let see = static_exchange_eval(board, ctx.atk, m);
            if see > 0 {
                ctx.stats.see_win_caps += 1;
            } else if see == 0 {
                ctx.stats.see_equal_caps += 1;
            } else {
                ctx.stats.see_loss_caps += 1;
                if move_flags(m) != MF_PROMOTION {
                    continue;
                }
                ctx.stats.see_loss_searched += 1;
            }
        }

        let undo = board.make_move(m, ctx.z);
        if board.is_in_check(board.side.flip()) {
            board.unmake_move(m, &undo, ctx.z);
            continue;
        }

        let score = -quiescence(board, ctx, -beta, -alpha, ply + 1, qs_ply + 1);
        board.unmake_move(m, &undo, ctx.z);

        if score >= beta {
            return score;
        }
        if score > alpha {
            alpha = score;
        }
    }

    alpha
}

fn score_moves(
    board: &mut Board,
    ctx: &SearchContext,
    list: &mut MoveList,
    tt_move: Move,
    ply: usize,
) {
    let us = board.side as usize;
    let prev_move = if ply > 0 && ply < 128 {
        ctx.stack[ply - 1].current_move
    } else {
        MOVE_NONE
    };
    let counter = if prev_move != MOVE_NONE {
        ctx.counter[move_from(prev_move) as usize][move_to(prev_move) as usize]
    } else {
        MOVE_NONE
    };

    for i in 0..list.count {
        list.scores[i] = score_single_move(
            board,
            ctx,
            list.moves[i],
            MoveScoreContext {
                tt_move,
                ply,
                counter,
                us,
            },
        );
    }
}

fn score_captures(board: &Board, ctx: &SearchContext, list: &mut MoveList) {
    let us = board.side as usize;
    for i in 0..list.count {
        let m = list.moves[i];
        let cap = board.sq_piece[move_to(m) as usize];
        let cap_val = if cap != PIECE_NONE {
            piece_type(cap).material_value()
        } else {
            100
        };
        let mov_val = piece_type(board.sq_piece[move_from(m) as usize]).material_value();
        let mover_pt = piece_type(board.sq_piece[move_from(m) as usize]) as usize;
        let cap_pt = if cap != PIECE_NONE {
            piece_type(cap) as usize
        } else {
            0
        };
        let to = move_to(m) as usize;
        let ch = ctx.cap_history[us][mover_pt][to][cap_pt] / CAP_HISTORY_DIVISOR;
        let see = if ctx.options.search.see && ctx.options.search.see_capture_ordering {
            static_exchange_eval(board, ctx.atk, m)
        } else {
            0
        };
        list.scores[i] = see * 16 + cap_val * 10 - mov_val + ch;
    }
}

// ============================================================
// Section 5: Heuristic helpers
// ============================================================

fn update_killers(ctx: &mut SearchContext, ply: usize, m: Move) {
    if ply >= MAX_PLY {
        return;
    }
    if ctx.killers[ply][0] != m {
        ctx.killers[ply][1] = ctx.killers[ply][0];
        ctx.killers[ply][0] = m;
    }
}

fn history_delta(depth: i32) -> i32 {
    depth * depth
}

fn add_history_score(
    ctx: &mut SearchContext,
    color: Color,
    moving_piece: Piece,
    m: Move,
    delta: i32,
) {
    if moving_piece == PIECE_NONE {
        return;
    }
    let pt = piece_type(moving_piece) as usize;
    let to = move_to(m) as usize;
    let ci = color as usize;
    ctx.history[ci][pt][to] += delta;
    if ctx.history[ci][pt][to].abs() > HISTORY_OVERFLOW_THRESHOLD {
        scale_down_history(ctx, ci);
    }
}

fn scale_down_history(ctx: &mut SearchContext, ci: usize) {
    for piece_history in &mut ctx.history[ci] {
        for v in piece_history.iter_mut() {
            *v /= 2;
        }
    }
}

fn scale_down_cap_history(ctx: &mut SearchContext, ci: usize) {
    for pt in 0..6 {
        for sq in 0..64 {
            for cpt in 0..6 {
                ctx.cap_history[ci][pt][sq][cpt] /= 2;
            }
        }
    }
}

fn update_cap_history(ctx: &mut SearchContext, color: Color, m: Move, board: &Board, depth: i32) {
    let ci = color as usize;
    let mover = board.sq_piece[move_from(m) as usize];
    if mover == PIECE_NONE {
        return;
    }
    let mover_pt = piece_type(mover) as usize;
    let to = move_to(m) as usize;
    let cap = board.sq_piece[move_to(m) as usize];
    let cap_pt = if cap != PIECE_NONE {
        piece_type(cap) as usize
    } else {
        0
    };
    let bonus = depth * depth;
    ctx.cap_history[ci][mover_pt][to][cap_pt] += bonus;
    if ctx.cap_history[ci][mover_pt][to][cap_pt] > HISTORY_OVERFLOW_THRESHOLD {
        scale_down_cap_history(ctx, ci);
    }
}

fn compute_lmr_reduction_details(input: LmrInput, ctx: &mut SearchContext) -> LmrReduction {
    if input.moves_searched < LMR_FULL_DEPTH_MOVES
        || input.depth < LMR_MIN_DEPTH
        || input.is_capture
        || input.is_promo
        || input.gives_check
        || input.in_check
    {
        return LmrReduction {
            base_reduction: 0,
            final_reduction: 0,
        };
    }
    ctx.stats.lmr_attempts += 1;
    let move_count = input.moves_searched - LMR_FULL_DEPTH_MOVES + 1;
    let depth_ln = (input.depth as f64).ln();
    let move_ln = (move_count as f64).ln();
    let mut reduction = (0.5 + depth_ln * move_ln / LMR_LOG_DIVISOR).floor() as i32;
    let base_reduction = reduction;

    let history_bonus =
        input.history_score.max(0).clamp(0, LMR_HISTORY_CLAMP) / LMR_HISTORY_NORMALIZER;
    reduction -= history_bonus;

    if LMR_NODE_TYPE_SCALING {
        if input.is_pv {
            reduction = (reduction * 3 + 3) / 4;
        } else if input.is_cut_node {
            reduction = (reduction * 23 + 10) / 20;
        }
    }

    if input.improving {
        reduction += LMR_IMPROVING_BONUS;
    }

    let pre_protection_reduction = reduction.clamp(0, input.depth - 2);
    if pre_protection_reduction > 0
        && criticality_score(input, base_reduction, pre_protection_reduction)
            >= CRITICALITY_P99_LOGIT
    {
        reduction -= 1;
    }

    LmrReduction {
        base_reduction,
        final_reduction: reduction.clamp(0, input.depth - 2),
    }
}

fn criticality_score(input: LmrInput, base_reduction: i32, final_reduction: i32) -> f64 {
    let new_depth = if final_reduction > 0 {
        (input.depth - 1 - final_reduction).max(1)
    } else {
        input.depth - 1
    };
    let prev_static_eval = input.prev_static_eval.unwrap_or(0);
    let static_eval_delta = input
        .prev_static_eval
        .map_or(0, |prev| input.static_eval - prev);
    let piece = if input.moving_piece == PIECE_NONE {
        PieceType::None
    } else {
        piece_type(input.moving_piece)
    };

    CRITICALITY_INTERCEPT
        + 0.622_035_682_254_059_8 * (input.root_depth as f64 / 16.0)
        + 1.469_181_333_356_005_5 * (input.ply as f64 / 32.0)
        - 0.965_000_546_499_052_1 * (input.depth as f64 / 16.0)
        - 1.083_317_076_398_084_6 * (input.move_index as f64 / 32.0)
        - 1.620_795_533_159_597 * (base_reduction as f64 / 4.0)
        + 2.112_306_951_495_513_3 * (final_reduction as f64 / 4.0)
        - 2.773_046_292_446_39 * (new_depth as f64 / 16.0)
        + 0.314_789_997_221_647_8 * normalized_history(input.history_score)
        + 7.845_683_391_975_632 * normalized_score(input.static_eval)
        - 0.246_839_894_777_142_55 * bool_feature(input.prev_static_eval.is_some())
        + 1.894_434_140_212_677 * normalized_score(prev_static_eval)
        + 4.112_301_192_541_191 * normalized_score(static_eval_delta)
        - 7.828_034_647_662_448 * normalized_score(input.alpha)
        - 0.551_107_766_850_325_7 * normalized_score(input.beta)
        + 0.689_816_954_641_299_9 * bool_feature(input.is_pv)
        - 0.689_816_954_641_299_9 * bool_feature(input.is_cut_node)
        + 0.246_564_314_784_833_87 * bool_feature(input.improving)
        + 2.496_561_329_270_951_6 * bool_feature(input.is_counter)
        + 0.000_064_818_428_349_186_92 * bool_feature(input.side_to_move == Color::Black)
        + 0.318_342_357_499_804_05 * bool_feature(piece == PieceType::Pawn)
        - 0.674_590_255_178_052_2 * bool_feature(piece == PieceType::Knight)
        + 0.053_494_108_574_306_07 * bool_feature(piece == PieceType::Bishop)
        + 0.014_231_863_750_547_216 * bool_feature(piece == PieceType::Rook)
        - 0.296_776_996_024_212_7 * bool_feature(piece == PieceType::Queen)
        + 0.107_945_921_490_359_38 * bool_feature(piece == PieceType::King)
        // The trained weights for is_killer and tt_move_agreement are exactly zero.
        + 0.0 * bool_feature(input.is_killer)
        + 0.0 * bool_feature(input.tt_move_agreement)
}

fn normalized_score(score: Score) -> f64 {
    score.clamp(-2_000, 2_000) as f64 / 2_000.0
}

fn normalized_history(history_score: i32) -> f64 {
    history_score.clamp(-16_384, 16_384) as f64 / 16_384.0
}

fn bool_feature(value: bool) -> f64 {
    if value {
        1.0
    } else {
        0.0
    }
}

fn is_improving(ctx: &SearchContext, static_eval: Score, ply: usize) -> bool {
    if ply < 2 || ply - 2 >= MAX_PLY {
        return false;
    }
    ctx.stack[ply - 2]
        .static_eval
        .is_some_and(|prev_eval| static_eval > prev_eval)
}

fn build_criticality_record(
    ctx: &SearchContext,
    input: CriticalityRecordInput,
) -> Option<CriticalityRecord> {
    if !input.enabled || ctx.criticality_logger.is_none() {
        return None;
    }
    Some(CriticalityRecord {
        pid: std::process::id(),
        game_id: ctx.game_id,
        search_id: ctx.search_id,
        root_depth: ctx.root_depth,
        ply: input.ply,
        node_hash: input.node_hash,
        side_to_move: input.side_to_move,
        m: input.m,
        from: input.from,
        to: input.to,
        piece: input.moving_piece,
        depth: input.depth,
        move_index: input.move_index,
        base_reduction: input.base_reduction,
        final_reduction: input.final_reduction,
        new_depth: input.new_depth,
        history_score: input.history_score,
        static_eval: input.static_eval,
        prev_static_eval: input.prev_static_eval,
        alpha: input.alpha,
        beta: input.beta,
        is_pv: input.is_pv,
        is_cut_node: input.is_cut_node,
        improving: input.improving,
        is_killer: input.is_killer,
        is_counter: input.is_counter,
        tt_move_agreement: input.tt_move_agreement,
        label_source: CriticalityLabelSource::None,
        reduced_score: None,
        full_score: None,
    })
}

fn should_run_criticality_probe(
    ctx: &SearchContext,
    node_hash: u64,
    m: Move,
    depth: i32,
    ply: usize,
    reduction: i32,
    reduced_score: Score,
    alpha: Score,
) -> bool {
    reduction > 0
        && !ctx.in_criticality_probe
        && reduced_score <= alpha
        && ctx.criticality_logger.is_some()
        && should_probe_criticality(
            node_hash,
            m,
            depth,
            ply,
            ctx.search_id,
            ctx.options.criticality.probe_permille,
        )
}

fn write_criticality_record(ctx: &mut SearchContext, record: &CriticalityRecord) {
    let Some(logger) = &mut ctx.criticality_logger else {
        return;
    };
    if let Err(err) = logger.write(record) {
        eprintln!("info string criticality log write failed: {err}");
        ctx.criticality_logger = None;
    }
}

// ============================================================
// Section 7: Static Exchange Evaluation
// ============================================================

fn static_exchange_eval(board: &Board, atk: &AttackTables, m: Move) -> i32 {
    if m == MOVE_NONE {
        return 0;
    }

    let from = move_from(m);
    let to = move_to(m);
    let flags = move_flags(m);
    let mover = board.sq_piece[from as usize];
    if mover == PIECE_NONE {
        return 0;
    }

    let moving_side = board.side;
    let mover_type = piece_type(mover);
    let moved_type = if flags == MF_PROMOTION {
        move_promo_pt(m)
    } else {
        mover_type
    };
    let captured_value = captured_value_for_see(board, m);
    let promotion_gain = if flags == MF_PROMOTION {
        moved_type.material_value() - PieceType::Pawn.material_value()
    } else {
        0
    };

    let mut pieces = board.pieces;
    let mut occ = board.occ_all;

    pieces[moving_side as usize][mover_type as usize] &= !bb(from);
    occ &= !bb(from);

    if flags == MF_EN_PASSANT {
        let cap_sq = if moving_side == Color::White {
            to - 8
        } else {
            to + 8
        };
        pieces[moving_side.flip() as usize][PieceType::Pawn as usize] &= !bb(cap_sq);
        occ &= !bb(cap_sq);
    } else {
        let captured = board.sq_piece[to as usize];
        if captured != PIECE_NONE {
            pieces[piece_color(captured) as usize][piece_type(captured) as usize] &= !bb(to);
        }
        occ &= !bb(to);
    }

    pieces[moving_side as usize][moved_type as usize] |= bb(to);
    occ |= bb(to);

    let mut gain = [0i32; 32];
    gain[0] = captured_value + promotion_gain;

    let mut depth = 0usize;
    let mut side = moving_side.flip();
    let mut victim_side = moving_side;
    let mut victim_type = moved_type;
    let mut victim_value = moved_type.material_value();
    let target_bb = bb(to);

    while depth + 1 < gain.len() {
        let Some((attacker_sq, attacker_type)) =
            least_valuable_attacker(to, side, occ, &pieces, atk)
        else {
            break;
        };

        let attacker_bb = bb(attacker_sq);
        pieces[victim_side as usize][victim_type as usize] &= !target_bb;
        pieces[side as usize][attacker_type as usize] &= !attacker_bb;
        pieces[side as usize][attacker_type as usize] |= target_bb;
        occ &= !attacker_bb;

        if attacker_type == PieceType::King
            && attackers_to(to, side.flip(), occ, &pieces, atk)
                & color_occupancy(&pieces, side.flip())
                != 0
        {
            break;
        }

        depth += 1;
        gain[depth] = victim_value - gain[depth - 1];
        victim_side = side;
        victim_type = attacker_type;
        victim_value = attacker_type.material_value();
        side = side.flip();
    }

    while depth > 0 {
        depth -= 1;
        gain[depth] = -gain[depth + 1].max(-gain[depth]);
    }

    gain[0]
}

fn captured_value_for_see(board: &Board, m: Move) -> i32 {
    if move_flags(m) == MF_EN_PASSANT {
        return PieceType::Pawn.material_value();
    }
    let captured = board.sq_piece[move_to(m) as usize];
    if captured == PIECE_NONE {
        return 0;
    }
    piece_type(captured).material_value()
}

fn least_valuable_attacker(
    target: Square,
    color: Color,
    occ: Bb,
    pieces: &[[Bb; 6]; 2],
    atk: &AttackTables,
) -> Option<(Square, PieceType)> {
    let attackers = attackers_to(target, color, occ, pieces, atk);
    if attackers == 0 {
        return None;
    }

    let ci = color as usize;
    for pt in [
        PieceType::Pawn,
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ] {
        let bb = attackers & pieces[ci][pt as usize];
        if bb != 0 {
            return Some((bb_lsb(bb), pt));
        }
    }

    None
}

fn attackers_to(
    target: Square,
    color: Color,
    occ: Bb,
    pieces: &[[Bb; 6]; 2],
    atk: &AttackTables,
) -> Bb {
    let ci = color as usize;
    let target_bb = bb(target);
    let pawn_attackers = if color == Color::White {
        crate::movegen::pawn_attacks_black(target_bb)
    } else {
        crate::movegen::pawn_attacks_white(target_bb)
    } & pieces[ci][PieceType::Pawn as usize];
    let knight_attackers = atk.knight[target as usize] & pieces[ci][PieceType::Knight as usize];
    let bishop_attackers = atk.bishop_attacks(target, occ)
        & (pieces[ci][PieceType::Bishop as usize] | pieces[ci][PieceType::Queen as usize]);
    let rook_attackers = atk.rook_attacks(target, occ)
        & (pieces[ci][PieceType::Rook as usize] | pieces[ci][PieceType::Queen as usize]);
    let king_attackers = atk.king[target as usize] & pieces[ci][PieceType::King as usize];

    pawn_attackers | knight_attackers | bishop_attackers | rook_attackers | king_attackers
}

fn color_occupancy(pieces: &[[Bb; 6]; 2], color: Color) -> Bb {
    pieces[color as usize].iter().fold(0, |occ, bb| occ | *bb)
}

// ============================================================
// Section 8: Draw detection helpers
// ============================================================

fn is_insufficient_material(board: &Board) -> bool {
    if board.occ_all.count_ones() == 2 {
        return true;
    }
    if board.occ_all.count_ones() == 3 {
        let bishops = board.pieces[0][PieceType::Bishop as usize]
            | board.pieces[1][PieceType::Bishop as usize];
        let knights = board.pieces[0][PieceType::Knight as usize]
            | board.pieces[1][PieceType::Knight as usize];
        if (bishops | knights).count_ones() == 1 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tt::TranspositionTable;
    use std::sync::atomic::AtomicBool;

    fn test_context<'a>(
        atk: &'a AttackTables,
        z: &'a Zobrist,
        tt: &'a mut TranspositionTable,
        limits: Limits,
        stop: &'a AtomicBool,
    ) -> SearchContext<'a> {
        SearchContext::new(
            atk,
            z,
            tt,
            limits,
            Vec::new(),
            0,
            EngineOptions::default(),
            None,
            stop,
            0,
            0,
        )
    }

    fn generated_move(board: &Board, atk: &AttackTables, uci: &str) -> Move {
        let list = gen_moves(board, atk);
        for i in 0..list.count {
            if move_name(list.moves[i]) == uci {
                return list.moves[i];
            }
        }
        panic!("move {uci} was not generated in {}", board.to_fen());
    }

    fn see_for(fen: &str, uci: &str) -> i32 {
        let atk = AttackTables::init();
        let board = Board::from_fen(fen).unwrap();
        let m = generated_move(&board, &atk, uci);
        static_exchange_eval(&board, &atk, m)
    }

    fn lmr_reduction_for(input: LmrInput) -> i32 {
        let atk = AttackTables::init();
        let z = Zobrist::new();
        let mut tt = TranspositionTable::new(1);
        let stop = AtomicBool::new(false);
        let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
        compute_lmr_reduction_details(input, &mut ctx).final_reduction
    }

    fn reducible_lmr_input(depth: i32, moves_searched: usize) -> LmrInput {
        LmrInput {
            moves_searched,
            move_index: moves_searched,
            ply: 0,
            depth,
            history_score: 0,
            static_eval: 0,
            prev_static_eval: None,
            alpha: 0,
            beta: 0,
            root_depth: depth,
            side_to_move: Color::White,
            moving_piece: make_piece(Color::White, PieceType::Pawn),
            is_pv: false,
            is_cut_node: false,
            improving: false,
            is_killer: false,
            is_counter: false,
            tt_move_agreement: false,
            is_capture: false,
            is_promo: false,
            gives_check: false,
            in_check: false,
        }
    }

    #[test]
    fn lmr_keeps_protected_moves_full_depth() {
        let mut input = reducible_lmr_input(8, LMR_FULL_DEPTH_MOVES);
        input.is_capture = true;
        assert_eq!(lmr_reduction_for(input), 0);

        let mut input = reducible_lmr_input(8, LMR_FULL_DEPTH_MOVES);
        input.gives_check = true;
        assert_eq!(lmr_reduction_for(input), 0);
    }

    #[test]
    fn lmr_scales_with_depth_and_move_count() {
        let shallow = lmr_reduction_for(reducible_lmr_input(5, LMR_FULL_DEPTH_MOVES + 3));
        let deep_late = lmr_reduction_for(reducible_lmr_input(12, LMR_FULL_DEPTH_MOVES + 16));
        assert!(deep_late > shallow);
    }

    #[test]
    fn lmr_base_formula_rounds_to_nearest() {
        assert_eq!(
            lmr_reduction_for(reducible_lmr_input(3, LMR_FULL_DEPTH_MOVES + 3)),
            1
        );
    }

    #[test]
    fn lmr_improving_is_logged_but_does_not_change_reduction() {
        let mut input = reducible_lmr_input(8, LMR_FULL_DEPTH_MOVES);
        assert_eq!(lmr_reduction_for(input), 0);
        input.improving = true;
        assert_eq!(lmr_reduction_for(input), 0);
    }

    #[test]
    fn lmr_uses_history_to_adjust_reduction() {
        let mut good_history = reducible_lmr_input(12, LMR_FULL_DEPTH_MOVES + 16);
        good_history.history_score = LMR_HISTORY_CLAMP;
        let mut bad_history = good_history;
        bad_history.history_score = -LMR_HISTORY_CLAMP;

        let neutral_history = lmr_reduction_for(reducible_lmr_input(12, LMR_FULL_DEPTH_MOVES + 16));
        assert_eq!(lmr_reduction_for(bad_history), neutral_history);
        assert!(lmr_reduction_for(good_history) < neutral_history);
    }

    #[test]
    fn lmr_applies_learned_criticality_p99_protection() {
        let baseline = reducible_lmr_input(12, LMR_FULL_DEPTH_MOVES + 16);
        let baseline_reduction = lmr_reduction_for(baseline);
        assert!(baseline_reduction > 0);

        let mut critical = baseline;
        critical.ply = 20;
        critical.static_eval = 2_000;
        critical.prev_static_eval = Some(-2_000);
        critical.alpha = -2_000;
        critical.beta = -1_900;
        critical.is_counter = true;

        assert!(
            criticality_score(critical, baseline_reduction, baseline_reduction)
                >= CRITICALITY_P99_LOGIT
        );
        assert_eq!(lmr_reduction_for(critical), baseline_reduction - 1);
    }

    #[test]
    fn quiet_history_updates_the_moving_side() {
        let atk = AttackTables::init();
        let z = Zobrist::new();
        let mut tt = TranspositionTable::new(1);
        let stop = AtomicBool::new(false);
        let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
        let mut board = Board::startpos();

        let white_move = generated_move(&board, &atk, "e2e4");
        handle_beta_cutoff(&mut ctx, &board, white_move, 1, 6, false);
        let white_pt = piece_type(board.sq_piece[move_from(white_move) as usize]) as usize;
        assert!(ctx.history[Color::White as usize][white_pt][move_to(white_move) as usize] > 0);
        assert_eq!(
            ctx.history[Color::Black as usize][white_pt][move_to(white_move) as usize],
            0
        );

        let undo = board.make_move(white_move, &z);
        assert_eq!(board.side, Color::Black);
        let black_move = generated_move(&board, &atk, "e7e5");
        let black_pt = piece_type(board.sq_piece[move_from(black_move) as usize]) as usize;
        handle_beta_cutoff(&mut ctx, &board, black_move, 1, 6, false);
        assert!(ctx.history[Color::Black as usize][black_pt][move_to(black_move) as usize] > 0);
        assert_eq!(
            ctx.history[Color::White as usize][black_pt][move_to(black_move) as usize],
            0
        );
        board.unmake_move(white_move, &undo, &z);
    }

    #[test]
    fn lmr_history_lookup_after_make_move_uses_the_mover_side() {
        let atk = AttackTables::init();
        let z = Zobrist::new();
        let mut tt = TranspositionTable::new(1);
        let stop = AtomicBool::new(false);
        let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
        let mut board = Board::startpos();
        let m = generated_move(&board, &atk, "e2e4");
        let moving_piece = board.sq_piece[move_from(m) as usize];

        add_history_score(&mut ctx, Color::White, moving_piece, m, 1234);
        let undo = board.make_move(m, &z);
        assert_eq!(board.side, Color::Black);

        let mover = board.side.flip() as usize;
        let history_score =
            ctx.history[mover][piece_type(moving_piece) as usize][move_to(m) as usize];
        assert_eq!(mover, Color::White as usize);
        assert_eq!(history_score, 1234);

        board.unmake_move(m, &undo, &z);
    }

    #[test]
    fn improving_compares_static_eval_two_plies_back() {
        let atk = AttackTables::init();
        let z = Zobrist::new();
        let mut tt = TranspositionTable::new(1);
        let stop = AtomicBool::new(false);
        let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
        let mut board = Board::startpos();

        let eval0 = evaluate(
            &board,
            &EvalContext {
                atk: &atk,
                options: &ctx.options,
            },
        );
        ctx.stack[0].static_eval = Some(eval0);
        assert!(!is_improving(&ctx, eval0, 0));
        assert!(!is_improving(&ctx, -eval0, 1));

        let null_undo = board.make_null_move(&z);
        let null_eval = evaluate(
            &board,
            &EvalContext {
                atk: &atk,
                options: &ctx.options,
            },
        );
        assert_eq!(board.side, Color::Black);
        assert!(!is_improving(&ctx, null_eval, 1));
        ctx.stack[1].static_eval = Some(null_eval);
        board.unmake_null_move(&null_undo);

        let white_move = generated_move(&board, &atk, "e2e4");
        let undo_white = board.make_move(white_move, &z);
        let black_move = generated_move(&board, &atk, "e7e5");
        let undo_black = board.make_move(black_move, &z);
        assert_eq!(board.side, Color::White);

        let eval2 = evaluate(
            &board,
            &EvalContext {
                atk: &atk,
                options: &ctx.options,
            },
        );
        assert_eq!(is_improving(&ctx, eval2, 2), eval2 > eval0);

        board.unmake_move(black_move, &undo_black, &z);
        board.unmake_move(white_move, &undo_white, &z);
    }

    #[test]
    fn quiet_history_distribution_is_not_immediately_saturated() {
        let atk = AttackTables::init();
        let z = Zobrist::new();
        let mut tt = TranspositionTable::new(16);
        let stop = AtomicBool::new(false);
        let mut ctx = test_context(
            &atk,
            &z,
            &mut tt,
            Limits {
                max_depth: 6,
                ..Limits::default()
            },
            &stop,
        );
        let mut board = Board::startpos();
        ctx.root_color = board.side;
        let mut pv = Vec::new();
        let _ = alpha_beta(
            &mut board,
            &mut ctx,
            SearchNode {
                alpha: -SCORE_INF,
                beta: SCORE_INF,
                depth: 6,
                ply: 0,
                is_pv: true,
            },
            &mut pv,
        );

        let mut white_abs_sum = 0i64;
        let mut black_abs_sum = 0i64;
        let mut max_abs = 0i32;
        let mut nonzero = 0usize;
        for pt in 0..6 {
            for to in 0..64 {
                let white = ctx.history[Color::White as usize][pt][to].abs();
                let black = ctx.history[Color::Black as usize][pt][to].abs();
                white_abs_sum += white as i64;
                black_abs_sum += black as i64;
                max_abs = max_abs.max(white).max(black);
                nonzero += usize::from(white != 0) + usize::from(black != 0);
            }
        }

        eprintln!(
            "history distribution: white_abs_sum={white_abs_sum} black_abs_sum={black_abs_sum} max_abs={max_abs} nonzero={nonzero}"
        );

        assert!(nonzero > 0);
        assert!(max_abs < LMR_HISTORY_CLAMP);
        assert!(white_abs_sum > 0);
        assert!(black_abs_sum > 0);
    }

    #[test]
    fn see_scores_clean_capture_at_full_victim_value() {
        assert_eq!(
            see_for("4k3/8/8/8/3q4/8/3R4/4K3 w - - 0 1", "d2d4"),
            PieceType::Queen.material_value()
        );
    }

    #[test]
    fn see_scores_profitable_capture_after_recapture() {
        assert_eq!(
            see_for("r2qk3/8/8/8/8/8/8/3RK3 w - - 0 1", "d1d8"),
            PieceType::Queen.material_value() - PieceType::Rook.material_value()
        );
    }

    #[test]
    fn see_rejects_losing_minor_for_pawn_capture() {
        assert_eq!(
            see_for("4k3/5p2/4p3/8/2B5/8/8/4K3 w - - 0 1", "c4e6"),
            PieceType::Pawn.material_value() - PieceType::Bishop.material_value()
        );
    }

    #[test]
    fn see_scores_even_rook_trade_as_zero() {
        assert_eq!(see_for("r2qk3/8/8/8/8/8/8/R3K3 w - - 0 1", "a1a8"), 0);
    }

    #[test]
    fn see_handles_en_passant_captured_pawn_square() {
        assert_eq!(see_for("3rk3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", "e5d6"), 0);
    }

    #[test]
    fn see_includes_promotion_material_gain() {
        assert_eq!(
            see_for("1r2k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7b8q"),
            PieceType::Rook.material_value() + PieceType::Queen.material_value()
                - PieceType::Pawn.material_value()
        );
    }

    #[test]
    fn node_limit_is_checked_without_4096_node_granularity() {
        let atk = AttackTables::init();
        let z = Zobrist::new();
        let mut tt = TranspositionTable::new(1);
        let stop = AtomicBool::new(false);
        let limits = Limits {
            max_depth: 8,
            nodes: 1,
            ..Limits::default()
        };
        let mut ctx = test_context(&atk, &z, &mut tt, limits, &stop);
        let mut board = Board::startpos();

        let result = search(&mut board, &mut ctx);

        assert_eq!(result.nodes, 1);
        assert_eq!(result.depth, 0);
        assert!(ctx.stopped);
        assert_ne!(result.best_move, MOVE_NONE);
    }

    #[test]
    fn quiescence_reports_checkmate_instead_of_standing_pat() {
        let atk = AttackTables::init();
        let z = Zobrist::new();
        let mut tt = TranspositionTable::new(1);
        let stop = AtomicBool::new(false);
        let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
        let mut board = Board::from_fen("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1").unwrap();

        let score = quiescence(&mut board, &mut ctx, -SCORE_INF, SCORE_INF, 0, 0);
        assert_eq!(score, -SCORE_MATE);
    }

    #[test]
    fn depth_zero_in_check_detects_mate() {
        let atk = AttackTables::init();
        let z = Zobrist::new();
        let mut tt = TranspositionTable::new(1);
        let stop = AtomicBool::new(false);
        let mut board = Board::from_fen("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1").unwrap();
        let mut ctx = test_context(
            &atk,
            &z,
            &mut tt,
            Limits {
                max_depth: 1,
                ..Limits::default()
            },
            &stop,
        );
        ctx.root_color = board.side;

        let mut pv = Vec::new();
        let score = alpha_beta(
            &mut board,
            &mut ctx,
            SearchNode {
                alpha: -SCORE_INF,
                beta: SCORE_INF,
                depth: 0,
                ply: 0,
                is_pv: true,
            },
            &mut pv,
        );

        assert_eq!(score, -SCORE_MATE);
    }
}
