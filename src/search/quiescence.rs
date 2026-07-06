use super::*;
use crate::{probe, sample_probe};
#[cfg(feature = "probes")]
use crate::tt::bound_str;

pub(in crate::search) fn quiescence(
    board: &mut Board,
    ctx: &mut SearchContext,
    mut alpha: Score,
    beta: Score,
    ply: usize,
) -> Score {
    if ctx.should_stop() {
        return 0;
    }
    ctx.nodes += 1;
    ctx.stats.qnodes += 1;

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

    let original_alpha = alpha; // Keep original alpha for TT cutoff check
    let hash = board.hash;
    let tt_entry_option = ctx.tt.probe(hash);
    let mut tt_move = MOVE_NONE;
    let mut raw_eval_from_tt = None;

    if let Some(entry) = tt_entry_option {
        tt_move = entry.best;
        raw_eval_from_tt = Some(entry.raw_eval);

        // Qsearch depth is 0, so any TT entry depth >= 0 is sufficient
        let depth_sufficient = entry.depth >= 0;

        // TT Cutoff check
        let mut do_cutoff = false;
        let mut cutoff_score = 0;

        if depth_sufficient {
            let adjusted_score = score_from_tt(entry.score, ply);
            if entry.bound == Bound::Exact {
                cutoff_score = adjusted_score;
                do_cutoff = true;
            } else if entry.bound == Bound::Lower && adjusted_score >= beta {
                cutoff_score = adjusted_score;
                do_cutoff = true;
            } else if entry.bound == Bound::Upper && adjusted_score <= alpha {
                cutoff_score = adjusted_score;
                do_cutoff = true;
            }
        }

        if do_cutoff {
            probe!(
                TtCutoff,
                TtCutoffEvent {
                    depth: 0,
                    entry_type: bound_str(entry.bound),
                    entry_depth: entry.depth,
                    depth_sufficient: depth_sufficient,
                    cutoff_score: cutoff_score,
                    alpha: original_alpha,
                    beta: beta,
                }
            );
            return cutoff_score;
        }
    }

    if board.is_in_check(board.side) {
        // Search all check evasions without a ply cap — evasion sequences are
        // naturally bounded (perpetual check is caught by repetition).
        // MAX_PLY is the only hard limit.
        if ply >= MAX_PLY {
            return evaluate(
                board,
                &EvalContext {
                    atk: ctx.atk,
                    options: &ctx.options,
                },
            );
        }

        let mut list = gen_moves(board, ctx.atk);
        score_moves(board, ctx, &mut list, tt_move, ply);

        let mut best_move = tt_move;

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
            ctx.history_hashes.push(hash);
            let score = -quiescence(board, ctx, -beta, -alpha, ply + 1);
            ctx.history_hashes.pop();
            board.unmake_move(m, &undo, ctx.z);

            if ctx.stopped {
                return 0;
            }
            if score >= beta {
                ctx.tt.store(hash, score_to_tt(score, ply), m, 0, Bound::Lower, raw_eval_from_tt.unwrap_or(0));
                return score;
            }
            if score > alpha {
                alpha = score;
                best_move = m;
            }
        }

        if legal_moves == 0 {
            let final_score = -(SCORE_MATE - ply as Score);
            ctx.tt.store(hash, score_to_tt(final_score, ply), MOVE_NONE, 0, Bound::Exact, raw_eval_from_tt.unwrap_or(0));
            return final_score;
        }
        ctx.tt.store(hash, score_to_tt(alpha, ply), best_move, 0, get_bound(alpha, original_alpha, beta), raw_eval_from_tt.unwrap_or(0));
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
    let raw_eval = raw_eval_from_tt.unwrap_or(stand_pat as i16);

    let mut best_move = tt_move;

    if stand_pat >= beta {
        ctx.tt.store(hash, score_to_tt(stand_pat, ply), MOVE_NONE, 0, Bound::Lower, raw_eval);
        return stand_pat;
    }
    if stand_pat > alpha {
        alpha = stand_pat;
        best_move = MOVE_NONE;
    }

    let mut list = gen_captures(board, ctx.atk);

    score_captures(board, ctx, &mut list, tt_move);

    let mut _captures_searched: u32 = 0;
    let mut _delta_pruned: u32 = 0;
    let mut _see_pruned: u32 = 0;

    for i in 0..list.count {
        list.pick_best(i);
        let m = list.moves[i];

        // Delta pruning (only for captures, not for checks)
        let cap_piece = board.sq_piece[move_to(m) as usize];
        let is_capture = cap_piece != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
        let mut cap_value = if cap_piece != PIECE_NONE {
            piece_type(cap_piece).material_value()
        } else if move_flags(m) == MF_EN_PASSANT {
            PieceType::Pawn.material_value()
        } else {
            0
        };
        // Promotion-captures gain the promotion piece value on top of the
        // captured piece — the pawn is replaced by a queen/rook/bishop/knight.
        if move_flags(m) == MF_PROMOTION {
            cap_value += move_promo_pt(m).material_value() - PieceType::Pawn.material_value();
        }
        if is_capture && stand_pat + cap_value + DELTA_PRUNING_MARGIN < alpha {
            _delta_pruned += 1;
            continue;
        }

        if ctx.options.search.see && ctx.options.search.see_qsearch_pruning {
            let see = static_exchange_eval(board, ctx.atk, m);
            sample_probe!(
                16,
                See,
                SeeEvent {
                    see_value: see,
                    captured_value: 0,
                    threshold: 0,
                    pruned_by_see: see < 0 && move_flags(m) != MF_PROMOTION,
                    searched_despite_bad_see: see < 0 && move_flags(m) == MF_PROMOTION,
                    pin_excluded: false,
                }
            );
            if see > 0 {
                ctx.stats.see_win_caps += 1;
            } else if see == 0 {
                ctx.stats.see_equal_caps += 1;
            } else {
                ctx.stats.see_loss_caps += 1;
                if move_flags(m) != MF_PROMOTION {
                    _see_pruned += 1;
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

        ctx.history_hashes.push(hash);
        let score = -quiescence(board, ctx, -beta, -alpha, ply + 1);
        ctx.history_hashes.pop();
        board.unmake_move(m, &undo, ctx.z);

        _captures_searched += 1;
        if score >= beta {
            ctx.tt.store(hash, score_to_tt(score, ply), m, 0, Bound::Lower, raw_eval);
            return score;
        }
        if score > alpha {
            alpha = score;
            best_move = m;
        }
    }

    ctx.tt.store(hash, score_to_tt(alpha, ply), best_move, 0, get_bound(alpha, original_alpha, beta), raw_eval);
    sample_probe!(
        32,
        Quiescence,
        QuiescenceEvent {
            ply: ply as u32,
            stand_pat_score: stand_pat,
            alpha: original_alpha,
            beta: beta,
            final_score: alpha,
            captures_searched: _captures_searched,
            delta_pruned_count: _delta_pruned,
            see_pruned_count: _see_pruned,
            in_check: board.is_in_check(board.side),
            futility_cutoff: false,
        }
    );

    alpha
}

// Helper to determine the bound type for TT store
fn get_bound(score: Score, alpha: Score, beta: Score) -> Bound {
    if score >= beta {
        Bound::Lower
    } else if score <= alpha {
        Bound::Upper
    } else {
        Bound::Exact
    }
}
