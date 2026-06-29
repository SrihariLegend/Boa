use super::*;
use crate::{probe, probe_close, probe_open};
pub(super) struct GoContext<'a> {
    pub(super) board: &'a mut Board,
    pub(super) position_history: &'a [u64],
    pub(super) atk: &'a AttackTables,
    pub(super) z: &'a Zobrist,
    pub(super) tt: &'a TranspositionTable,
    pub(super) contempt: i32,
    pub(super) options: EngineOptions,
    pub(super) syzygy: Option<&'a SyzygyTablebase>,
    pub(super) stop_flag: &'a std::sync::atomic::AtomicBool,
    pub(super) game_id: u64,
    pub(super) search_id: u64,
}

pub(super) fn handle_go<'a>(tokens: impl Iterator<Item = &'a str>, go: GoContext<'_>) {
    let mut limits = Limits::default();
    let mut tok_iter = tokens.peekable();
    while let Some(tok) = tok_iter.next() {
        match tok {
            "depth" => {
                limits.max_depth = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(64);
            }
            "nodes" => {
                limits.nodes = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "movetime" => {
                limits.move_time = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "wtime" => {
                limits.wtime = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "btime" => {
                limits.btime = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "winc" => {
                limits.winc = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "binc" => {
                limits.binc = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "movestogo" => {
                limits.moves_to_go = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "infinite" => {
                limits.max_depth = 64;
            }
            "perft" => {
                let d: u32 = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(1);
                run_perft(go.board, go.atk, go.z, d);
                return;
            }
            _ => {}
        }
    }

    // Config event — once per search
    probe!(Config, ConfigEvent {
        tt_size_mb: go.tt.size_mb() as u32,
        material_scale: go.options.eval.material_scale,
        pst_scale: go.options.eval.pst_scale,
        mobility_scale: go.options.eval.mobility_scale,
        king_safety_scale: go.options.eval.king_safety_scale,
        pawn_structure_scale: go.options.eval.pawn_structure_scale,
        contempt: go.contempt,
        syzygy_enabled: go.syzygy.is_some(),
        max_depth: limits.max_depth,
        move_time: limits.move_time,
        wtime: limits.wtime,
        btime: limits.btime,
        winc: limits.winc,
        binc: limits.binc,
        moves_to_go: limits.moves_to_go,
    });

    let history_for_search =
        go.position_history[..go.position_history.len().saturating_sub(1)].to_vec();
    let mut ctx = SearchContext::new(
        go.atk,
        go.z,
        go.tt,
        limits,
        history_for_search,
        go.contempt,
        go.options,
        go.syzygy,
        go.stop_flag,
        go.game_id,
        go.search_id,
    );
    let result = search(go.board, &mut ctx);
    println!("bestmove {}", move_name(result.best_move));
    let _ = io::stdout().flush();
}
