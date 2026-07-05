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
    let r = NULL_MOVE_BASE_R + depth / NULL_MOVE_DEPTH_DIVISOR;
    let null_depth = depth - r;
    let undo = board.make_null_move(ctx.z);
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
        },
        &mut null_pv,
    );
    board.unmake_null_move(&undo);

    let pruned = null_score >= beta;
    probe!(
        NullMove,
        NullMoveEvent {
            depth: depth,
            static_eval: static_eval,
            beta: beta,
            reduction: r,
            null_move_score: null_score,
            pruned: pruned,
        }
    );
    if pruned {
        ctx.stats.null_move_cutoffs += 1;
        return Some(beta);
    }
    None
}
