use super::*;
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
    pub(in crate::search) in_criticality_probe: bool,

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
    pub(in crate::search) smp_worker_id: usize,

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

    pub(in crate::search) fn should_stop(&mut self) -> bool {
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
    pub(in crate::search) fn time_for_move(&self, side: Color) -> (u64, u64) {
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
    pub(in crate::search) fn is_repetition(&self, board: &Board) -> bool {
        let lookback = (board.halfmove as usize).min(self.history_hashes.len());
        self.history_hashes
            .iter()
            .rev()
            .take(lookback)
            .any(|&h| h == board.hash)
    }
}

pub(in crate::search) fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ============================================================
// Section 0: Bench
