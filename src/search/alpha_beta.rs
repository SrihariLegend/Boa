use super::*;
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
    let original_alpha = alpha;

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
        if let Some(rfp_score) = rfp_prune_score(static_eval, beta, depth) {
            ctx.stats.rfp_cutoffs += 1;
            ctx.history_hashes.pop();
            return rfp_score;
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
    let mut ffp_pruned_any = false;

    for i in 0..list.count {
        list.pick_best(i);
        let m = list.moves[i];
        let node_hash = board.hash;
        let side_to_move = board.side;
        let from = move_from(m);
        let to = move_to(m);
        let moving_piece = board.sq_piece[from as usize];
        let is_capture =
            board.sq_piece[to as usize] != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
        let is_promo = move_flags(m) == MF_PROMOTION;
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
        let mover = board.side as usize;
        let history_score = if moving_piece != PIECE_NONE {
            ctx.history[mover][piece_type(moving_piece) as usize][to as usize]
        } else {
            0
        };
        let is_killer = ply < 128 && (m == ctx.killers[ply][0] || m == ctx.killers[ply][1]);
        let is_counter = m == counter_move;
        let ffp_see = if ctx.options.search.forward_futility_pruning
            && !ctx.in_criticality_probe
            && !is_pv
            && depth >= 1
            && depth <= FFP_MAX_DEPTH
            && !in_check
            && !is_capture
            && !is_promo
            && !is_mate_score(alpha)
            && !is_mate_score(static_eval)
            && m != tt_move
            && !is_killer
            && !is_counter
        {
            Some(static_exchange_eval(board, ctx.atk, m))
        } else {
            None
        };

        // Make move and verify legality
        let undo = board.make_move(m, ctx.z);
        if board.is_in_check(board.side.flip()) {
            board.unmake_move(m, &undo, ctx.z);
            continue;
        }
        legal_moves += 1;

        let gives_check = board.is_in_check(board.side);
        let is_quiet = !is_capture && !is_promo && !gives_check;
        let is_lmr_quiet = is_quiet;

        // ---- Forward futility pruning (quiet move frontier) ----
        if is_quiet && ffp_see.is_some_and(|see| see <= 0) {
            ctx.stats.ffp_attempts += 1;
            let quiet_move_index = (quiet_moves_searched + 1).min(FFP_MAX_RANK);
            let ffp_input = FfpInput {
                depth,
                static_eval,
                alpha,
                is_cut_node,
                move_index: quiet_move_index,
            };
            if should_ffp_prune(ffp_input) {
                ctx.stats.ffp_prunes += 1;
                ffp_pruned_any = true;
                if should_run_futility_probe(ctx, node_hash, m, depth, ply) {
                    let mut full_pv = Vec::new();
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
                            is_pv: false,
                        },
                        &mut full_pv,
                    );
                    ctx.in_criticality_probe = was_in_probe;
                    if !ctx.stopped {
                        let margin = ffp_margin(ffp_input);
                        write_criticality_record(
                            ctx,
                            &CriticalityRecord {
                                decision_kind: CriticalityDecisionKind::Futility,
                                pid: std::process::id(),
                                game_id: ctx.game_id,
                                search_id: ctx.search_id,
                                root_depth: ctx.root_depth,
                                ply,
                                node_hash,
                                side_to_move,
                                m,
                                from,
                                to,
                                piece: moving_piece,
                                depth,
                                move_index: quiet_move_index,
                                base_reduction: 0,
                                final_reduction: 0,
                                new_depth: depth - 1,
                                history_score,
                                static_eval,
                                prev_static_eval,
                                alpha,
                                beta,
                                futility_margin: Some(margin),
                                static_alpha_margin: Some(alpha - static_eval),
                                is_pv,
                                is_cut_node,
                                improving,
                                is_killer,
                                is_counter,
                                tt_move_agreement: m == tt_move,
                                label_source: CriticalityLabelSource::CounterfactualProbe,
                                reduced_score: Some(alpha),
                                full_score: Some(full_score),
                            },
                        );
                    }
                }
                board.unmake_move(m, &undo, ctx.z);
                continue;
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

    // If forward futility pruned every legal move, return the original upper
    // bound and avoid storing a bogus MOVE_NONE / -INF entry in the TT.
    if moves_searched == 0 {
        ctx.history_hashes.pop();
        return alpha;
    }

    // Pop our position hash from history
    ctx.history_hashes.pop();

    // If some legal moves were pruned and none of the searched moves raised
    // alpha, we only know the node failed low relative to the original window.
    // Returning/storing a searched best_score below original_alpha would create
    // an over-tight upper bound that ignores the unsearched pruned moves.
    if ffp_pruned_any && bound == Bound::Upper {
        best_score = best_score.max(original_alpha);
    }

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
