use super::*;

/// Try null-move pruning. Returns Some(score) if we can cut off.
pub(in crate::search) fn try_null_move(
    board: &mut Board,
    ctx: &mut SearchContext,
    beta: Score,
    depth: i32,
    ply: usize,
    static_eval: Score,
) -> Option<Score> {
    if depth < NULL_MOVE_MIN_DEPTH || static_eval < beta {
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

    if null_score >= beta {
        ctx.stats.null_move_cutoffs += 1;
        return Some(beta);
    }
    None
}
