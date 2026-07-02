use super::*;
use crate::sample_probe;

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
            let score = -quiescence(board, ctx, -beta, -alpha, ply + 1);
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

    let mut captures_searched: u32 = 0;
    let mut delta_pruned: u32 = 0;
    let mut see_pruned: u32 = 0;

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
            delta_pruned += 1;
            continue;
        }

        if ctx.options.search.see && ctx.options.search.see_qsearch_pruning {
            let see = static_exchange_eval(board, ctx.atk, m);
            sample_probe!(16, See, SeeEvent {
                see_value: see,
                captured_value: 0,
                threshold: 0,
                pruned_by_see: see < 0 && move_flags(m) != MF_PROMOTION,
                searched_despite_bad_see: see < 0 && move_flags(m) == MF_PROMOTION,
                pin_excluded: false,
            });
            if see > 0 {
                ctx.stats.see_win_caps += 1;
            } else if see == 0 {
                ctx.stats.see_equal_caps += 1;
            } else {
                ctx.stats.see_loss_caps += 1;
                if move_flags(m) != MF_PROMOTION {
                    see_pruned += 1;
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

        let score = -quiescence(board, ctx, -beta, -alpha, ply + 1);
        board.unmake_move(m, &undo, ctx.z);

        captures_searched += 1;
        if score >= beta {
            return score;
        }
        if score > alpha {
            alpha = score;
        }
    }

    sample_probe!(32, Quiescence, QuiescenceEvent {
        ply: ply as u32,
        stand_pat_score: stand_pat,
        alpha: alpha,
        beta: beta,
        final_score: alpha,
        captures_searched: captures_searched,
        delta_pruned_count: delta_pruned,
        see_pruned_count: see_pruned,
        in_check: false,
        futility_cutoff: false,
    });

    alpha
}
