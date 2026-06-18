// ============================================================
// search.rs — Alpha-beta search with Boa-style pruning policy
//
// Core algorithm: PVS (Principal Variation Search) with iterative deepening.
//
// Boa-style search modifications:
//   1. Move ordering: restriction score boosts moves that reduce opponent mobility
//   2. Positional mode detection: when position is "quiet" (low tactics),
//      we reduce LMR aggressiveness and allow quiet moves to breathe
//   3. Squeeze extensions: moves that drastically reduce opponent freedom
//      get a +1 ply extension
//   4. No null-move pruning in squeeze positions (null move misleads here)
//   5. Contempt: slight draw avoidance to prefer grinding
// ============================================================

use crate::board::{Board, Zobrist};
use crate::config::{scale_score, EngineOptions};
use crate::eval::{evaluate, EvalContext};
use crate::movegen::{gen_captures, gen_moves, AttackTables, MoveList};
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
const RFP_MARGIN_PER_DEPTH: i32 = 80;

/// RFP: maximum depth at which to apply.
/// SF applies up to depth ~7-8. 6 is conservative.
const RFP_MAX_DEPTH: i32 = 6;

/// Null-move pruning: minimum depth to attempt.
/// Standard: 3 (CPW, SF).
const NULL_MOVE_MIN_DEPTH: i32 = 3;

/// Null-move reduction: base + depth/4.
/// SF uses 4 + depth/6 post-tuning; 3 + depth/4 is a common simpler formula (CPW).
const NULL_MOVE_BASE_R: i32 = 3;
const NULL_MOVE_DEPTH_DIVISOR: i32 = 4;

/// Null-move: cap return above beta to avoid false mates from zugzwang.
/// This prevents null-move from returning an inflated score. [NEEDS TUNING]
const NULL_MOVE_BETA_CAP: i32 = 100;

/// Late move reductions: minimum moves searched before applying LMR.
/// SF uses 3-4 depending on context. 4 is standard (CPW).
const LMR_FULL_DEPTH_MOVES: usize = 4;

/// LMR: minimum depth to start reducing.
/// Standard: 3 (CPW).
const LMR_MIN_DEPTH: i32 = 3;

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

/// Squeeze mode: opponent mobility threshold.
/// When opponent has <= this many pseudo-legal moves, we consider them "squeezed".
/// Average mobility in chess is ~30-35 moves. 12 represents severely restricted. [NEEDS TUNING]
const SQUEEZE_MOBILITY_THRESHOLD: u32 = 12;

/// Restriction extension: quiet moves that cut opponent mobility by at least
/// this much are searched one ply deeper when the resulting mobility is low.
const RESTRICTION_EXTENSION_MOBILITY_DROP: u32 = 4;

/// Restriction extension: do not extend ordinary developing moves that merely
/// trim mobility from a still-free position.
const RESTRICTION_EXTENSION_MAX_MOBILITY: u32 = 18;

/// Move ordering: quiet moves that reduce opponent mobility should be searched
/// earlier even when the drop is not large enough to earn an extension.
const RESTRICTION_ORDER_DROP_BONUS: i32 = 120;
const RESTRICTION_ORDER_LOW_MOBILITY_BONUS: i32 = 12;
const RESTRICTION_ORDER_SQUEEZE_BONUS: i32 = 160;
const RESTRICTION_ORDER_COUNTERPLAY_PENALTY: i32 = 40;
const RESTRICTION_ORDER_MIN_DEPTH: i32 = 3;

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

    pub restriction_extensions: u64,
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
             iid {} iid_ok {} restrict_ext {}",
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
            self.restriction_extensions,
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
    opponent_mobility_before: Option<u32>,
}

#[derive(Clone, Copy)]
struct LmrInput {
    moves_searched: usize,
    depth: i32,
    is_capture: bool,
    is_promo: bool,
    gives_check: bool,
    in_check: bool,
    squeeze_mode: bool,
    squeeze_lmr_relief: bool,
    improving: bool,
}

// ---- Search context ----

pub struct SearchContext<'a> {
    pub atk: &'a AttackTables,
    pub z: &'a Zobrist,
    pub tt: &'a mut TranspositionTable,
    pub limits: Limits,
    pub start_ms: u64,
    pub contempt: i32,
    pub options: EngineOptions,
    pub root_color: Color,

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

    // Killer moves: [ply][slot]
    pub killers: [[Move; 2]; 128],

    // History heuristic: [color][from][to]
    pub history: [[[i32; 64]; 64]; 2],

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
    pub static_eval: Score,
}

impl<'a> SearchContext<'a> {
    pub fn new(
        atk: &'a AttackTables,
        z: &'a Zobrist,
        tt: &'a mut TranspositionTable,
        limits: Limits,
        history_hashes: Vec<u64>,
        contempt: i32,
        options: EngineOptions,
        stop_flag: &'a std::sync::atomic::AtomicBool,
    ) -> Self {
        SearchContext {
            atk,
            z,
            tt,
            limits,
            stop_flag,
            start_ms: now_ms(),
            root_depth: 0,
            contempt,
            options,
            root_color: Color::White, // set by search() before iterating
            history_hashes,
            nodes: 0,
            stopped: false,
            killers: [[MOVE_NONE; 2]; 128],
            history: [[[0i32; 64]; 64]; 2],
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
            &no_stop,
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
    ctx.tt.new_search();
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
    squeeze_mode: bool,
    suppress_in_squeeze: bool,
) -> Option<Score> {
    if (squeeze_mode && suppress_in_squeeze) || depth < NULL_MOVE_MIN_DEPTH || static_eval < beta {
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
        return Some(null_score.min(beta + NULL_MOVE_BETA_CAP));
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
    if is_capture {
        update_cap_history(ctx, board.side, m, board, depth);
        return;
    }
    update_killers(ctx, ply, m);
    update_history(ctx, board.side, m, depth);
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
    s += ctx.history[scoring.us][move_from(m) as usize][move_to(m) as usize];
    if ctx.options.search.restriction_ordering {
        s += scale_score(
            restriction_move_score(board, ctx, m, scoring.opponent_mobility_before),
            ctx.options.search.restriction_ordering_scale,
        );
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
            options: ctx.options,
        },
    );
    if ply < 128 {
        ctx.stack[ply].static_eval = static_eval;
    }
    let improving = (2..128).contains(&ply) && static_eval > ctx.stack[ply - 2].static_eval;

    // ---- Positional mode detection (the Boa adaptation) ----
    let squeeze_mode = !in_check && is_squeeze_position(board, ctx.atk);

    // ---- Pruning heuristics (skip in check, PV, squeeze mode) ----

    if !in_check && !is_pv {
        // Reverse futility pruning (static null move)
        // When improving, we use a tighter margin (position is getting better,
        // so we're less willing to prune). SF uses similar improving adjustments.
        let rfp_margin = if improving {
            RFP_MARGIN_PER_DEPTH * depth * 3 / 4
        } else {
            RFP_MARGIN_PER_DEPTH * depth
        };
        if depth <= RFP_MAX_DEPTH && static_eval - rfp_margin >= beta && !is_mate_score(static_eval)
        {
            ctx.stats.rfp_cutoffs += 1;
            ctx.history_hashes.pop();
            return static_eval - rfp_margin;
        }

        // Null move pruning — DISABLED in squeeze mode (critical Boa adaptation)
        if let Some(null_score) = try_null_move(
            board,
            ctx,
            beta,
            depth,
            ply,
            static_eval,
            squeeze_mode,
            ctx.options.search.squeeze_null_move_suppression,
        ) {
            ctx.history_hashes.pop();
            return null_score;
        }
    }

    // Generate and order moves
    let mut list = gen_moves(board, ctx.atk);
    score_moves(board, ctx, &mut list, tt_move, ply, depth);

    let mut best_move = MOVE_NONE;
    let mut best_score = -SCORE_INF;
    let mut bound = Bound::Upper;
    let mut moves_searched = 0;
    let mut legal_moves = 0;
    let pre_move_opponent_mobility = if !in_check {
        Some(side_mobility_for_search(board, ctx.atk, board.side.flip()))
    } else {
        None
    };

    for i in 0..list.count {
        list.pick_best(i);
        let m = list.moves[i];

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
        let _is_killer = ply < 128 && (m == ctx.killers[ply][0] || m == ctx.killers[ply][1]);

        // ---- Squeeze extension ----
        // Capped like the check extension (ply < root_depth + 2). Without the
        // cap, depth never decreases in stable squeezes (which persist by
        // design), so lines never reach quiescence and only terminate at
        // repetition/50-move draws — making the engine score *won* squeeze
        // endgames as draws (e.g. KQvK read as -contempt at depth 2+).
        let post_move_opponent_mobility = pre_move_opponent_mobility
            .map(|_| side_mobility_for_search(board, ctx.atk, board.side));
        let squeeze_ext = if ctx.options.search.squeeze_extensions
            && !is_capture
            && !is_promo
            && !in_check
            && ply < ply_limit
        {
            match (pre_move_opponent_mobility, post_move_opponent_mobility) {
                (Some(before), Some(after))
                    if should_extend_restriction(before, after, squeeze_mode) =>
                {
                    1
                }
                _ => 0,
            }
        } else {
            0
        };
        if squeeze_ext > 0 {
            ctx.stats.restriction_extensions += 1;
        }

        // ---- Late move reductions (LMR) ----
        let reduction = compute_lmr_reduction(
            LmrInput {
                moves_searched,
                depth,
                is_capture,
                is_promo,
                gives_check,
                in_check,
                squeeze_mode,
                squeeze_lmr_relief: ctx.options.search.squeeze_lmr_relief,
                improving,
            },
            ctx,
        );
        if reduction > 0 {
            ctx.stats.lmr_actual_reductions += 1;
        }

        let new_depth = (depth - 1 + squeeze_ext - reduction).max(0);

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
                    depth: depth - 1 + squeeze_ext,
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
                        depth: depth - 1 + squeeze_ext,
                        ply: ply + 1,
                        is_pv,
                    },
                    &mut child_pv,
                );
            }
            s
        };

        board.unmake_move(m, &undo, ctx.z);
        moves_searched += 1;

        if ctx.stopped {
            ctx.history_hashes.pop();
            return 0;
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

    // TT store (mate scores converted to node-relative distance)
    ctx.tt.store(
        board.hash,
        score_to_tt(best_score, ply),
        best_move,
        depth as i8,
        bound,
    );

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
                    options: ctx.options,
                },
            );
        }

        let mut list = gen_moves(board, ctx.atk);
        score_moves(board, ctx, &mut list, MOVE_NONE, ply, 0);

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
            options: ctx.options,
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
    depth: i32,
) {
    let us = board.side as usize;
    let opponent_mobility_before = if depth >= RESTRICTION_ORDER_MIN_DEPTH {
        Some(side_mobility_for_search(board, ctx.atk, board.side.flip()))
    } else {
        None
    };
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
                opponent_mobility_before,
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
        list.scores[i] = cap_val * 10 - mov_val + ch;
    }
}

/// Boa restriction bonus for move ordering.
fn restriction_move_score(
    board: &mut Board,
    ctx: &SearchContext,
    m: Move,
    opponent_mobility_before: Option<u32>,
) -> i32 {
    let mut score = restriction_shape_score(board, m);
    let Some(opponent_mobility_before) = opponent_mobility_before else {
        return score;
    };
    let flags = move_flags(m);
    if flags == MF_PROMOTION
        || flags == MF_EN_PASSANT
        || board.sq_piece[move_to(m) as usize] != PIECE_NONE
    {
        return score;
    }

    let undo = board.make_move(m, ctx.z);
    let legal = !board.is_in_check(board.side.flip());
    let opponent_mobility_after = if legal {
        Some(side_mobility_for_search(board, ctx.atk, board.side))
    } else {
        None
    };
    board.unmake_move(m, &undo, ctx.z);

    if let Some(after) = opponent_mobility_after {
        score += restriction_mobility_delta_score(opponent_mobility_before, after);
    }

    score
}

fn restriction_shape_score(board: &Board, m: Move) -> i32 {
    let to = move_to(m);
    let mover = board.sq_piece[move_from(m) as usize];
    if mover == PIECE_NONE {
        return 0;
    }
    let pt = piece_type(mover);
    let color = piece_color(mover);

    let centrality = centrality_score(to);
    let forward_bonus = if color == Color::White {
        sq_rank(to) as i32 * 2
    } else {
        (7 - sq_rank(to) as i32) * 2
    };
    let piece_bonus = match pt {
        PieceType::Knight => 15,
        PieceType::Bishop => 10,
        PieceType::Rook => 8,
        _ => 0,
    };

    centrality + forward_bonus + piece_bonus
}

fn restriction_mobility_delta_score(before: u32, after: u32) -> i32 {
    if after < before {
        let drop = before - after;
        let mut score = drop as i32 * RESTRICTION_ORDER_DROP_BONUS;
        if after <= RESTRICTION_EXTENSION_MAX_MOBILITY {
            score += (RESTRICTION_EXTENSION_MAX_MOBILITY - after) as i32
                * RESTRICTION_ORDER_LOW_MOBILITY_BONUS;
        }
        if after <= SQUEEZE_MOBILITY_THRESHOLD {
            score += RESTRICTION_ORDER_SQUEEZE_BONUS;
        }
        score
    } else if after > before {
        -((after - before).min(6) as i32 * RESTRICTION_ORDER_COUNTERPLAY_PENALTY)
    } else {
        0
    }
}

fn centrality_score(sq: Square) -> i32 {
    let f = sq_file(sq) as i32;
    let r = sq_rank(sq) as i32;
    let dist_f = (3 - f).abs().min((4 - f).abs());
    let dist_r = (3 - r).abs().min((4 - r).abs());
    let max_dist = dist_f + dist_r;
    (6 - max_dist).max(0) * 5
}

// ============================================================
// Section 5: Boa-specific search helpers
// ============================================================

fn is_squeeze_position(board: &Board, atk: &AttackTables) -> bool {
    side_mobility_for_search(board, atk, board.side.flip()) <= SQUEEZE_MOBILITY_THRESHOLD
}

fn side_mobility_for_search(board: &Board, atk: &AttackTables, color: Color) -> u32 {
    let ci = color as usize;
    let oi = color.flip() as usize;
    let occ = board.occ_all;
    let our_occ = board.occ[ci];

    let mut mobility = 0u32;

    let pawns = board.pieces[ci][PieceType::Pawn as usize];
    if color == Color::White {
        mobility += ((pawns << 8) & !occ).count_ones();
        mobility += (((pawns << 8) & !occ & BB_RANK_3) << 8 & !occ).count_ones();
        mobility += ((pawns << 9) & !BB_FILE_A & board.occ[oi]).count_ones();
        mobility += ((pawns << 7) & !BB_FILE_H & board.occ[oi]).count_ones();
    } else {
        mobility += ((pawns >> 8) & !occ).count_ones();
        mobility += (((pawns >> 8) & !occ & BB_RANK_6) >> 8 & !occ).count_ones();
        mobility += ((pawns >> 7) & !BB_FILE_A & board.occ[oi]).count_ones();
        mobility += ((pawns >> 9) & !BB_FILE_H & board.occ[oi]).count_ones();
    }

    let mut pieces = board.pieces[ci][PieceType::Knight as usize];
    while pieces != 0 {
        let sq = bb_pop_lsb(&mut pieces);
        mobility += (atk.knight[sq as usize] & !our_occ).count_ones();
    }

    let mut pieces = board.pieces[ci][PieceType::Bishop as usize];
    while pieces != 0 {
        let sq = bb_pop_lsb(&mut pieces);
        mobility += (atk.bishop_attacks(sq, occ) & !our_occ).count_ones();
    }

    let mut pieces = board.pieces[ci][PieceType::Rook as usize];
    while pieces != 0 {
        let sq = bb_pop_lsb(&mut pieces);
        mobility += (atk.rook_attacks(sq, occ) & !our_occ).count_ones();
    }

    let mut pieces = board.pieces[ci][PieceType::Queen as usize];
    while pieces != 0 {
        let sq = bb_pop_lsb(&mut pieces);
        mobility += (atk.queen_attacks(sq, occ) & !our_occ).count_ones();
    }

    let king_sq = board.king_sq[ci];
    if king_sq != NO_SQUARE {
        mobility += (atk.king[king_sq as usize] & !our_occ).count_ones();
    }

    mobility
}

fn should_extend_restriction(before: u32, after: u32, squeeze_mode: bool) -> bool {
    if after > before {
        return false;
    }
    if squeeze_mode && after <= SQUEEZE_MOBILITY_THRESHOLD {
        return true;
    }
    before - after >= RESTRICTION_EXTENSION_MOBILITY_DROP
        && after <= RESTRICTION_EXTENSION_MAX_MOBILITY
}

// ============================================================
// Section 6: Heuristic helpers
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

fn update_history(ctx: &mut SearchContext, color: Color, m: Move, depth: i32) {
    let from = move_from(m) as usize;
    let to = move_to(m) as usize;
    let ci = color as usize;
    let bonus = depth * depth;
    ctx.history[ci][from][to] += bonus;
    if ctx.history[ci][from][to] > HISTORY_OVERFLOW_THRESHOLD {
        for arr in &mut ctx.history[ci] {
            for v in arr.iter_mut() {
                *v /= 2;
            }
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

/// Compute LMR reduction for a move. Returns 0 if LMR doesn't apply.
fn compute_lmr_reduction(input: LmrInput, ctx: &mut SearchContext) -> i32 {
    if input.moves_searched < LMR_FULL_DEPTH_MOVES
        || input.depth < LMR_MIN_DEPTH
        || input.is_capture
        || input.is_promo
        || input.gives_check
        || input.in_check
    {
        return 0;
    }
    ctx.stats.lmr_attempts += 1;
    let mut base_r = lmr_reduction(input.depth, input.moves_searched);
    // Boa adaptation: reduce less in squeeze (positional grinding)
    if input.squeeze_mode && input.squeeze_lmr_relief {
        base_r = (base_r - 1).max(0);
    }
    // When position is not improving, search less deeply (SF-style)
    if !input.improving {
        base_r += 1;
    }
    base_r
}

fn lmr_reduction(depth: i32, moves_done: usize) -> i32 {
    let d = (depth as f32).ln();
    let m = (moves_done as f32).ln();
    (d * m / 2.0) as i32
}

// ============================================================
// Section 7: Draw detection helpers
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
            stop,
        )
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

    #[test]
    fn restriction_extension_rewards_large_drop_into_low_mobility() {
        assert!(should_extend_restriction(22, 18, false));
        assert!(should_extend_restriction(16, 12, false));
    }

    #[test]
    fn restriction_extension_ignores_small_or_still_free_drops() {
        assert!(!should_extend_restriction(22, 19, false));
        assert!(!should_extend_restriction(40, 32, false));
        assert!(!should_extend_restriction(12, 13, true));
    }

    #[test]
    fn restriction_ordering_rewards_mobility_drops() {
        assert!(restriction_mobility_delta_score(24, 20) > 0);
        assert!(
            restriction_mobility_delta_score(24, 12) > restriction_mobility_delta_score(24, 20)
        );
    }

    #[test]
    fn restriction_ordering_penalizes_releasing_counterplay() {
        assert_eq!(restriction_mobility_delta_score(16, 16), 0);
        assert!(restriction_mobility_delta_score(16, 20) < 0);
    }

    #[test]
    fn squeeze_mode_keeps_extending_lockdown_positions() {
        assert!(should_extend_restriction(12, 12, true));
        assert!(should_extend_restriction(8, 5, true));
    }
}
