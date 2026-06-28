use super::*;
pub fn search(board: &mut Board, ctx: &mut SearchContext) -> SearchResult {
    let threads = ctx.options.search.threads.clamp(1, 64);
    if ctx.options.search.lazy_smp && threads > 1 && ctx.limits.nodes == 0 {
        return lazy_smp_search(board, ctx, threads);
    }
    search_single(board, ctx, true, true)
}

pub(in crate::search) fn lazy_smp_search(
    board: &mut Board,
    ctx: &mut SearchContext,
    threads: usize,
) -> SearchResult {
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

pub(in crate::search) fn search_single(
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

pub(in crate::search) fn aspiration_search(
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
        if window_expand >= ASPIRATION_MAX_EXPANSIONS {
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
