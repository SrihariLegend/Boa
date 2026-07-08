use super::*;
use crate::probe;

/// Try null-move pruning. Returns Some(score) if we can cut off.
///
/// When correction is large (eval unreliable), the entry threshold is
/// tightened by |corr| * CORR_W_NMP / 512 — requiring eval to be further
/// above beta before attempting null move.
pub(in crate::search) fn try_null_move(
    board: &mut Board,
    ctx: &mut SearchContext,
    beta: Score,
    depth: i32,
    ply: usize,
    static_eval: Score,
    corr_val: i32,
) -> Option<Score> {
    let corr_margin = (corr_w_nmp() * corr_val.abs()) / 512;
    if depth < NULL_MOVE_MIN_DEPTH || static_eval - corr_margin < beta {
        return None;
    }
    let our_pieces = board.occ[board.side as usize]
        & !board.pieces[board.side as usize][PieceType::Pawn as usize]
        & !board.pieces[board.side as usize][PieceType::King as usize];
    if our_pieces == 0 {
        return None;
    }
    ctx.stats.null_move_tries += 1;
    let prev_move_was_tactical = if ply > 0 {
        ctx.stack[ply - 1].is_tactical
    } else {
        false
    };

    let eval_margin = (static_eval - beta) / 200;
    let eval_term = eval_margin.clamp(0, 3);

    let mut r = 4 + depth / 3 + eval_term;
    if prev_move_was_tactical {
        r += 1;
    }

    let null_depth = depth - r;
    let undo = board.make_null_move(ctx.z);

    if ply < 128 {
        ctx.stack[ply].current_move = MOVE_NONE;
        ctx.stack[ply].cont_entry = None;
    }

    let mut null_pv = Vec::new();
    let null_score = -alpha_beta(
        board,
        ctx,
        SearchNode {
            alpha: -beta,
            beta: -beta + 1,
            depth: null_depth,
            ply: ply + 1,
            is_pv: false,
            excluded_move: None,
        },
        &mut null_pv,
    );
    board.unmake_null_move(&undo);

    let pruned = null_score >= beta;
    let mut verified = pruned;

    if pruned && depth >= 14 {
        ctx.nmp_in_progress = true;
        let v_depth = depth - r - 4;
        let mut v_pv = Vec::new();

        ctx.history_hashes.pop(); // temporarily remove so verification search doesn't instantly draw
        let v_score = alpha_beta(
            board,
            ctx,
            SearchNode {
                alpha: beta - 1,
                beta,
                depth: v_depth,
                ply,
                is_pv: false,
                excluded_move: None,
            },
            &mut v_pv,
        );
        ctx.history_hashes.push(board.hash); // restore

        ctx.nmp_in_progress = false;
        if v_score < beta {
            verified = false;
        }
    }

    probe!(
        NullMove,
        NullMoveEvent {
            depth: depth,
            static_eval: static_eval,
            beta: beta,
            reduction: r,
            null_move_score: null_score,
            pruned: verified,
        }
    );
    if verified {
        ctx.stats.null_move_cutoffs += 1;
        return Some(beta);
    }
    None
}
