use super::*;
use crate::probe;

pub(in crate::search) fn alpha_beta(
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

    if ply >= MAX_PLY {
        return evaluate(
            board,
            &EvalContext {
                atk: ctx.atk,
                options: &ctx.options,
            },
        );
    }

    ctx.nodes += 1;
    ctx.stats.nodes += 1;

    // Draw detection: repetition, 50-move, insufficient material
    if ply > 0 {
        if board.halfmove >= 100 || is_insufficient_material(board) {
            // Contempt: positive from root side's view (root side avoids draws)
            let sign = if board.side == ctx.root_color { 1 } else { -1 };
            let score = SCORE_DRAW - ctx.contempt * sign;
            probe!(
                DrawDetection,
                DrawEvent {
                    draw_type: if board.halfmove >= 100 {
                        "fifty_move"
                    } else {
                        "insufficient_material"
                    },
                    ply: ply as u32,
                    contempt_applied: ctx.contempt * sign,
                    score_returned: score,
                }
            );
            return score;
        }
        // Repetition detection — check against ancestors (not self)
        if ctx.is_repetition(board) {
            let sign = if board.side == ctx.root_color { 1 } else { -1 };
            let score = SCORE_DRAW - ctx.contempt * sign;
            probe!(
                DrawDetection,
                DrawEvent {
                    draw_type: "repetition",
                    ply: ply as u32,
                    contempt_applied: ctx.contempt * sign,
                    score_returned: score,
                }
            );
            return score;
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
    let oa = alpha;
    #[allow(unused_variables)]
    let _ = oa;
    let ob = beta;
    #[allow(unused_variables)]
    let _ = ob;
    let mut alpha = alpha.max(-(SCORE_MATE - ply as Score));
    let beta_md = beta.min(SCORE_MATE - ply as Score - 1);
    if alpha >= beta_md {
        probe!(
            MateDistance,
            MateDistanceEvent {
                ply: ply as u32,
                original_alpha: oa,
                clamped_alpha: alpha,
                original_beta: ob,
                clamped_beta: beta_md,
                pruned: true,
            }
        );
        ctx.history_hashes.pop();
        return alpha;
    }
    probe!(
        MateDistance,
        MateDistanceEvent {
            ply: ply as u32,
            original_alpha: oa,
            clamped_alpha: alpha,
            original_beta: ob,
            clamped_beta: beta_md,
            pruned: false,
        }
    );
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
        return quiescence(board, ctx, alpha, beta, ply);
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
    let (mut tt_move, tt_cutoff, tt_raw_eval) =
        try_tt_cutoff(ctx, board.hash, depth, alpha, beta, is_pv, ply);
    if let Some(s) = tt_cutoff {
        ctx.history_hashes.pop();
        return s;
    }

    // ---- Internal Iterative Deepening (IID) ----
    // When we have no TT move at a PV node, do a reduced-depth search to
    // populate the TT with a candidate best move. Cut nodes usually get a
    // hash hit from a sibling and are skipped. [NEEDS SPRT]
    if tt_move == MOVE_NONE && depth >= IID_MIN_DEPTH && !in_check && !is_cut_node {
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
            probe!(
                Iid,
                IidEvent {
                    depth: depth,
                    reduced_depth: iid_depth,
                    tt_move_found_after_iid: true,
                    iid_search_score: 0,
                }
            );
        } else {
            probe!(
                Iid,
                IidEvent {
                    depth: depth,
                    reduced_depth: iid_depth,
                    tt_move_found_after_iid: false,
                    iid_search_score: 0,
                }
            );
        }
    }

    // Static evaluation for pruning heuristics — reuse TT raw_eval if available
    let mut static_eval = 0;
    if !in_check {
        static_eval = if let Some(re) = tt_raw_eval {
            if re != 0 {
                re as Score
            } else {
                evaluate(
                    board,
                    &EvalContext {
                        atk: ctx.atk,
                        options: &ctx.options,
                    },
                )
            }
        } else {
            evaluate(
                board,
                &EvalContext {
                    atk: ctx.atk,
                    options: &ctx.options,
                },
            )
        };
    }
    // Compute and cache non-pawn hashes for correction history.
    if ply < MAX_PLY {
        let np_hash_w = non_pawn_hash(board, ctx.z, Color::White);
        let np_hash_b = non_pawn_hash(board, ctx.z, Color::Black);
        ctx.stack[ply].non_pawn_hashes = Some((np_hash_w, np_hash_b));
    }

    // Compute correction value — used to widen pruning margins when eval
    // is unreliable for this position type. The raw static_eval feeds
    // pruning; |corr|/512 is added to each margin as an uncertainty term.
    let corr_val = if !in_check {
        compute_correction(ctx, board, ply)
    } else {
        0
    };
    if ply < MAX_PLY {
        ctx.stack[ply].correction_value = Some(corr_val);
    }
    let improving = if !in_check {
        is_improving(ctx, static_eval, ply)
    } else {
        false
    };
    if ply < MAX_PLY {
        ctx.stack[ply].static_eval = if !in_check { Some(static_eval) } else { None };
    }
    // ---- Pruning heuristics (skip in check and PV nodes) ----

    if !in_check && !is_pv {
        // Reverse futility pruning (static null move)
        if let Some(rfp_score) = rfp_prune_score(static_eval, beta, depth, corr_val) {
            ctx.stats.rfp_cutoffs += 1;
            ctx.history_hashes.pop();
            return rfp_score;
        }

        // Razoring
        if depth <= 4 && static_eval + 150 * depth <= alpha {
            let qscore = quiescence(board, ctx, alpha, beta, ply);
            if qscore <= alpha {
                ctx.history_hashes.pop();
                return qscore;
            }
        }

        let prev_move_was_null = ply > 0 && ctx.stack[ply - 1].current_move == MOVE_NONE;

        if !prev_move_was_null && !ctx.nmp_in_progress {
            if let Some(null_score) =
                try_null_move(board, ctx, beta, depth, ply, static_eval, corr_val)
            {
                ctx.history_hashes.pop();
                return null_score;
            }
        }
    }

    // Generate and order moves
    let mut list = gen_moves(board, ctx.atk);

    probe!(
        Movegen,
        MovegenEvent {
            total_count: list.count as u32,
            quiet_count: 0,
            capture_count: 0,
            evasion_count: 0,
            promotion_count: 0,
            in_check: board.is_in_check(board.side),
        }
    );

    score_moves(board, ctx, &mut list, tt_move, ply);
    // Lazy SMP: non-primary workers search different root moves first.
    // 3M dominates HISTORY_GRAVITY (16k) and any killer score
    // so the per-worker root move sorts first. Move-ordering scores are
    // unrelated to search bounds (SCORE_INF = 1_000_000), so 3M is safe here.
    if ply == 0 && ctx.smp_worker_id > 0 && list.count > 1 {
        let idx = ctx.smp_worker_id % list.count;
        list.scores[idx] += 3_000_000;
    }

    // ProbCut
    if !is_pv && !in_check && depth >= 5 && !is_mate_score(beta) {
        let prob_beta = beta + 150;

        let mut prob_tt_valid = true;
        if let Some(tt) = ctx.tt.probe(board.hash) {
            let s = score_from_tt(tt.score, ply);
            if tt.depth >= (depth - 4) as i8 && tt.bound == Bound::Upper && s < prob_beta {
                prob_tt_valid = false;
            }
        }

        if prob_tt_valid {
            let mut attempts = 0;
            let mut prob_cutoff = false;
            let mut final_prob_score = None;
            for i in 0..list.count {
                let m = list.moves[i];
                let to = move_to(m);
                let is_capture =
                    board.sq_piece[to as usize] != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;

                if is_capture && static_exchange_eval(board, ctx.atk, m) >= prob_beta - static_eval
                {
                    attempts += 1;
                    if ply < 128 {
                        ctx.stack[ply].current_move = m;
                        ctx.stack[ply].is_tactical = true;
                    }
                    let undo = board.make_move(m, ctx.z);
                    let mut prob_pv = Vec::new();
                    let prob_score = -alpha_beta(
                        board,
                        ctx,
                        SearchNode {
                            alpha: -prob_beta,
                            beta: -prob_beta + 1,
                            depth: depth - 4,
                            ply: ply + 1,
                            is_pv: false,
                        },
                        &mut prob_pv,
                    );
                    board.unmake_move(m, &undo, ctx.z);

                    final_prob_score = Some(prob_score);

                    if prob_score >= prob_beta {
                        prob_cutoff = true;
                        break;
                    }
                }
            }
            if attempts > 0 {
                #[allow(unused_variables)]
                let accepted = prob_cutoff;
                #[allow(unused_variables)]
                let prob_score = final_prob_score;
                #[allow(unused_variables)]
                let nodes_saved = if prob_cutoff { Some(0) } else { None };
                probe!(
                    ProbCut,
                    ProbCutEvent {
                        depth: depth,
                        beta: beta,
                        prob_beta: prob_beta,
                        static_eval: static_eval,
                        attempts: attempts,
                        accepted: accepted,
                        prob_score: prob_score,
                        nodes_saved: nodes_saved,
                    }
                );
                if prob_cutoff {
                    ctx.history_hashes.pop();
                    return prob_beta;
                }
            }
        }
    }

    let mut best_move = MOVE_NONE;
    let mut best_score = -SCORE_INF;
    let mut bound = Bound::Upper;
    let mut moves_searched = 0;
    let mut quiet_moves_searched = 0;
    let mut legal_moves = 0;
    let mut ffp_pruned_any = false;

    for i in 0..list.count {
        list.pick_best(i);
        let m = list.moves[i];
        let side_to_move = board.side;
        let from = move_from(m);
        let to = move_to(m);
        let moving_piece = board.sq_piece[from as usize];
        let is_capture =
            board.sq_piece[to as usize] != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
        let is_promo = move_flags(m) == MF_PROMOTION;
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
        let is_killer = ply < 128 && (m == ctx.killers[ply][0] || m == ctx.killers[ply][1]);
        let is_counter = m == counter_move;

        let mut history_score = if !is_capture { list.scores[i] } else { 0 };
        if history_score >= 700_000 {
            if m == tt_move {
                history_score = 0; // unknown history, assume 0 for LMR
            } else if ply < 128 && m == ctx.killers[ply][0] {
                history_score -= 900_000;
            } else if ply < 128 && m == ctx.killers[ply][1] {
                history_score -= 800_000;
            } else if is_counter {
                history_score -= 750_000;
            }
        }

        let ffp_eligible = ctx.options.search.forward_futility_pruning
            && !is_pv
            && (1..=FFP_MAX_DEPTH).contains(&depth)
            && !in_check
            && !is_capture
            && !is_promo
            && !is_mate_score(alpha)
            && !is_mate_score(static_eval)
            && m != tt_move
            && !is_killer
            && !is_counter;

        // Make move and verify legality
        let undo = board.make_move(m, ctx.z);
        if board.is_in_check(board.side.flip()) {
            board.unmake_move(m, &undo, ctx.z);
            continue;
        }
        legal_moves += 1;

        // Set continuation history entry for the child to read (ply+1 reads stack[ply]).
        // Quiet moves propagate their piece+to to the child; captures and promotions
        // must clear the entry so the child doesn't see a stale value from a prior
        // sibling branch.
        if ply < MAX_PLY {
            if moving_piece != PIECE_NONE && !is_capture && !is_promo {
                ctx.stack[ply].cont_entry = Some((piece_type(moving_piece) as usize, to as usize));
            } else {
                ctx.stack[ply].cont_entry = None;
            }
        }

        let gives_check = board.is_in_check(board.side);
        let is_quiet = !is_capture && !is_promo && !gives_check;
        // LMR excludes checks (tactically volatile), but history updates include
        // them — a failed check is still evidence the move was bad here.
        let is_lmr_quiet = is_quiet;
        let is_history_quiet = !is_capture && !is_promo;

        // ---- Forward futility pruning (quiet move frontier) ----
        if is_quiet && ffp_eligible {
            ctx.stats.ffp_attempts += 1;
            let quiet_move_index = (quiet_moves_searched + 1).min(FFP_MAX_RANK);
            let ffp_input = FfpInput {
                depth,
                static_eval,
                alpha,
                is_cut_node,
                move_index: quiet_move_index,
                history_score,
                corr_val,
            };
            if should_ffp_prune(ffp_input) {
                ctx.stats.ffp_prunes += 1;
                ffp_pruned_any = true;
                board.unmake_move(m, &undo, ctx.z);
                continue;
            }
        }

        // ---- History Pruning (5.5) ----
        if is_quiet && !is_pv && depth < 4 {
            if history_score < -5000 * depth {
                board.unmake_move(m, &undo, ctx.z);
                continue;
            }
            // Continuation pruning
            if ply >= 2 && ply < 128 {
                if let (Some((p1, to1)), Some((p2, to2))) =
                    (ctx.stack[ply - 1].cont_entry, ctx.stack[ply - 2].cont_entry)
                {
                    let mover_pt = piece_type(moving_piece) as usize;
                    let to_idx = to as usize;
                    let c1 = ctx.cont1[p1][to1][mover_pt][to_idx];
                    let c2 = ctx.cont2[p2][to2][mover_pt][to_idx];
                    if c1 < -2000 && c2 < -2000 {
                        // "strongly negative" threshold, we use -2000 roughly
                        board.unmake_move(m, &undo, ctx.z);
                        continue;
                    }
                }
            }
        }

        // ---- Late move reductions (LMR) ----
        let lmr = compute_lmr_reduction_details(
            LmrInput {
                moves_searched: quiet_moves_searched,
                move_index: moves_searched + 1,
                ply,
                depth,
                history_score,
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
                corr_val,
            },
            ctx,
        );
        let reduction = lmr.final_reduction;
        if reduction > 0 {
            ctx.stats.lmr_actual_reductions += 1;
        }

        let new_depth = if reduction > 0 {
            (depth - 1 - reduction).max(0)
        } else {
            depth - 1
        };
        let pre_alpha = alpha;

        // Record the move being searched BEFORE recursing — children read
        // stack[ply].current_move for the counter-move heuristic.
        if ply < 128 {
            ctx.stack[ply].current_move = m;
            ctx.stack[ply].is_tactical = is_capture || is_promo;
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
            }
            s
        };

        board.unmake_move(m, &undo, ctx.z);
        moves_searched += 1;
        if is_lmr_quiet {
            quiet_moves_searched += 1;
        }

        if ctx.stopped {
            ctx.history_hashes.pop();
            return 0;
        }

        if is_history_quiet && score <= pre_alpha {
            add_history_score(ctx, side_to_move, moving_piece, m, history_malus(depth));
            // Apply malus to pawn history for failed quiet moves
            {
                let pawn_idx = (board.pawn_hash & 1023) as usize;
                let pt = piece_type(moving_piece) as usize;
                let to = move_to(m) as usize;
                let old = ctx.pawn_history[pawn_idx][pt][to];
                let malus = history_malus(depth);
                ctx.pawn_history[pawn_idx][pt][to] =
                    old + malus - (old * malus.abs()) / HISTORY_GRAVITY;
            }
            // Apply malus to continuation history for failed quiet moves
            if ply > 0 && ply < MAX_PLY {
                if let Some((pp, pto)) = ctx.stack[ply - 1].cont_entry {
                    let pt = piece_type(moving_piece) as usize;
                    let to_idx = move_to(m) as usize;
                    let old = ctx.cont1[pp][pto][pt][to_idx];
                    let malus = history_malus(depth);
                    ctx.cont1[pp][pto][pt][to_idx] =
                        old + malus - (old * malus.abs()) / HISTORY_GRAVITY;
                }
            }
            // Cont2 malus with half magnitude
            if ply >= 2 && ply < MAX_PLY {
                if let Some((pp2, pto2)) = ctx.stack[ply - 2].cont_entry {
                    let pt = piece_type(moving_piece) as usize;
                    let to_idx = move_to(m) as usize;
                    let old = ctx.cont2[pp2][pto2][pt][to_idx];
                    let half_malus = history_malus(depth) / 2;
                    ctx.cont2[pp2][pto2][pt][to_idx] =
                        old + half_malus - (old * half_malus.abs()) / HISTORY_GRAVITY;
                }
            }
            // Cont4 malus with quarter magnitude
            if ply >= 4 && ply < MAX_PLY {
                if let Some((pp4, pto4)) = ctx.stack[ply - 4].cont_entry {
                    let pt = piece_type(moving_piece) as usize;
                    let to_idx = move_to(m) as usize;
                    let old = ctx.cont4[pp4][pto4][pt][to_idx];
                    let quarter_malus = history_malus(depth) / 4;
                    ctx.cont4[pp4][pto4][pt][to_idx] =
                        old + quarter_malus - (old * quarter_malus.abs()) / HISTORY_GRAVITY;
                }
            }
            // Cont6 malus with quarter magnitude
            if ply >= 6 && ply < MAX_PLY {
                if let Some((pp6, pto6)) = ctx.stack[ply - 6].cont_entry {
                    let pt = piece_type(moving_piece) as usize;
                    let to_idx = move_to(m) as usize;
                    let old = ctx.cont6[pp6][pto6][pt][to_idx];
                    let quarter_malus = history_malus(depth) / 4;
                    ctx.cont6[pp6][pto6][pt][to_idx] =
                        old + quarter_malus - (old * quarter_malus.abs()) / HISTORY_GRAVITY;
                }
            }
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
            handle_beta_cutoff(ctx, board, m, ply, depth, is_capture, score, beta);
            break;
        }
    }

    // Checkmate or stalemate
    if legal_moves == 0 {
        ctx.history_hashes.pop();
        let sign = if board.side == ctx.root_color { 1 } else { -1 };
        let final_score = if in_check {
            -(SCORE_MATE - ply as Score)
        } else {
            SCORE_DRAW - ctx.contempt * sign
        };
        ctx.tt.store(
            board.hash,
            score_to_tt(final_score, ply),
            MOVE_NONE,
            depth as i8,
            Bound::Exact,
            static_eval as i16,
        );
        return final_score;
    }

    // If forward futility pruned every legal move, return the original upper
    // bound and avoid storing a bogus MOVE_NONE / -INF entry in the TT.
    if moves_searched == 0 {
        ctx.history_hashes.pop();
        return alpha;
    }

    // Returning/storing a searched best_score below original_alpha would create
    // an over-tight upper bound that ignores the unsearched pruned moves.
    if ffp_pruned_any && bound == Bound::Upper {
        best_score = best_score.max(oa);
    }

    // Pop our position hash from history
    ctx.history_hashes.pop();

    // Update correction history using the search result vs raw eval.
    if !in_check {
        let raw_eval_for_correction = ctx.stack[ply].static_eval.unwrap_or(static_eval);
        update_correction(ctx, board, depth, best_score, raw_eval_for_correction, ply);
        ctx.stats.corr_update_count += 1;
    }

    // TT store (mate scores converted to node-relative distance).
    ctx.tt.store(
        board.hash,
        score_to_tt(best_score, ply),
        best_move,
        depth as i8,
        bound,
        static_eval as i16,
    );

    best_score
}
