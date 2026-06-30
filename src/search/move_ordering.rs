use super::*;

pub(in crate::search) fn score_moves(
    board: &mut Board,
    ctx: &SearchContext,
    list: &mut MoveList,
    tt_move: Move,
    ply: usize,
) {
    let us = board.side as usize;
    let prev_move = if ply > 0 && ply < 128 {
        ctx.stack[ply - 1].current_move
    } else {
        MOVE_NONE
    };
    let counter = if prev_move != MOVE_NONE {
        ctx.counter[move_from(prev_move) as usize][move_to(prev_move) as usize]
    } else {
        MOVE_NONE
    };

    for i in 0..list.count {
        list.scores[i] = score_single_move(board, ctx, list.moves[i], tt_move, ply, counter, us);
    }
}

/// Score a single move for move ordering.
fn score_single_move(
    board: &Board,
    ctx: &SearchContext,
    m: Move,
    tt_move: Move,
    ply: usize,
    counter: Move,
    us: usize,
) -> i32 {
    if m == tt_move {
        return 2_000_000;
    }
    if move_flags(m) == MF_PROMOTION {
        return 1_800_000 + move_promo_pt(m).material_value();
    }

    let cap = board.sq_piece[move_to(m) as usize];
    let is_capture = cap != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
    if is_capture {
        let cap_val = if cap != PIECE_NONE {
            piece_type(cap).material_value()
        } else {
            100
        };
        let mov_val = piece_type(board.sq_piece[move_from(m) as usize]).material_value();
        let mover_pt = piece_type(board.sq_piece[move_from(m) as usize]) as usize;
        let cap_pt = if cap != PIECE_NONE {
            piece_type(cap) as usize
        } else {
            0
        };
        let to = move_to(m) as usize;
        let ch = ctx.cap_history[us][mover_pt][to][cap_pt] / CAP_HISTORY_DIVISOR;
        return 1_000_000 + cap_val * 10 - mov_val + ch;
    }

    // Quiet move scoring
    let mut s = 0i32;
    if ply < 128 && m == ctx.killers[ply][0] {
        s += 900_000;
    } else if ply < 128 && m == ctx.killers[ply][1] {
        s += 800_000;
    } else if m == counter {
        s += 750_000;
    }
    let mover = board.sq_piece[move_from(m) as usize];
    if mover != PIECE_NONE {
        s += ctx.history[us][piece_type(mover) as usize][move_to(m) as usize];
    }
    // Continuation history (1-ply): if the previous move exists, look up
    // cont1[prev_piece][prev_to][current_piece][current_to].
    if ply > 0 && ply < 128 {
        if let Some((pp, pto)) = ctx.stack[ply - 1].cont_entry {
            let mover_pt = piece_type(mover) as usize;
            let to_idx = move_to(m) as usize;
            s += ctx.cont1[pp][pto][mover_pt][to_idx];
        }
    }
    // Continuation history 2-ply: 0.7× weight relative to offset 1.
    // Read from stack[ply-2].cont_entry (grandparent's move).
    if ply >= 2 && ply < 128 {
        if let Some((pp2, pto2)) = ctx.stack[ply - 2].cont_entry {
            let mover_pt = piece_type(mover) as usize;
            let to_idx = move_to(m) as usize;
            // 0.7x weight: multiply then divide
            s += (ctx.cont2[pp2][pto2][mover_pt][to_idx] * 7) / 10;
        }
    }
    // Continuation history offset 4: quarter weight
    if ply >= 4 && ply < 128 {
        if let Some((pp4, pto4)) = ctx.stack[ply - 4].cont_entry {
            let mover_pt = piece_type(mover) as usize;
            let to_idx = move_to(m) as usize;
            s += ctx.cont4[pp4][pto4][mover_pt][to_idx] / 4;
        }
    }
    // Continuation history offset 6: quarter weight
    if ply >= 6 && ply < 128 {
        if let Some((pp6, pto6)) = ctx.stack[ply - 6].cont_entry {
            let mover_pt = piece_type(mover) as usize;
            let to_idx = move_to(m) as usize;
            s += ctx.cont6[pp6][pto6][mover_pt][to_idx] / 4;
        }
    }
    s
}

pub(in crate::search) fn score_captures(board: &Board, ctx: &SearchContext, list: &mut MoveList) {
    let us = board.side as usize;
    for i in 0..list.count {
        let m = list.moves[i];
        let cap = board.sq_piece[move_to(m) as usize];
        let cap_val = if cap != PIECE_NONE {
            piece_type(cap).material_value()
        } else {
            100
        };
        let mov_val = piece_type(board.sq_piece[move_from(m) as usize]).material_value();
        let mover_pt = piece_type(board.sq_piece[move_from(m) as usize]) as usize;
        let cap_pt = if cap != PIECE_NONE {
            piece_type(cap) as usize
        } else {
            0
        };
        let to = move_to(m) as usize;
        let ch = ctx.cap_history[us][mover_pt][to][cap_pt] / CAP_HISTORY_DIVISOR;
        let see = if ctx.options.search.see && ctx.options.search.see_capture_ordering {
            static_exchange_eval(board, ctx.atk, m)
        } else {
            0
        };
        list.scores[i] = see * 16 + cap_val * 10 - mov_val + ch;
    }
}

// ============================================================
// Section 5: Heuristic helpers
// ============================================================

pub(in crate::search) fn update_killers(ctx: &mut SearchContext, ply: usize, m: Move) {
    if ply >= MAX_PLY {
        return;
    }
    if ctx.killers[ply][0] != m {
        ctx.killers[ply][1] = ctx.killers[ply][0];
        ctx.killers[ply][0] = m;
    }
}

/// History bonus for the best quiet move on a beta cutoff.
/// Obsidian-style linear+cap formula: (175 * d + 15).min(1409).
/// `is_strong_cutoff` adds 1 to depth when best_score > beta + 75,
/// indicating a genuinely strong move worth a larger bonus.
pub(in crate::search) fn history_delta(depth: i32, is_strong_cutoff: bool) -> i32 {
    let d = depth + if is_strong_cutoff { 1 } else { 0 };
    (175 * d + 15).min(1409)
}

/// Malus (negative bonus) applied to quiet moves that were searched
/// but failed to cause a beta cutoff. Obsidian-style formula with
/// slightly larger magnitude than the bonus for asymmetry.
pub(in crate::search) fn history_malus(depth: i32) -> i32 {
    -(196 * depth - 25).min(1047).max(-1047)
}

pub(in crate::search) fn add_history_score(
    ctx: &mut SearchContext,
    color: Color,
    moving_piece: Piece,
    m: Move,
    delta: i32,
) {
    if moving_piece == PIECE_NONE {
        return;
    }
    let pt = piece_type(moving_piece) as usize;
    let to = move_to(m) as usize;
    let ci = color as usize;
    let old = ctx.history[ci][pt][to];
    // Gravity formula: new = old + delta - old * abs(delta) / GRAVITY
    ctx.history[ci][pt][to] = old + delta - (old * delta.abs()) / HISTORY_GRAVITY;
}

pub(in crate::search) fn update_cap_history(
    ctx: &mut SearchContext,
    color: Color,
    m: Move,
    board: &Board,
    depth: i32,
) {
    let ci = color as usize;
    let mover = board.sq_piece[move_from(m) as usize];
    if mover == PIECE_NONE {
        return;
    }
    let mover_pt = piece_type(mover) as usize;
    let to = move_to(m) as usize;
    let cap = board.sq_piece[move_to(m) as usize];
    let cap_pt = if cap != PIECE_NONE {
        piece_type(cap) as usize
    } else {
        0
    };
    // Capture history uses the same bonus formula — captures that cause
    // beta cutoffs are inherently strong, so is_strong_cutoff is always true.
    let bonus = history_delta(depth, true);
    let old = ctx.cap_history[ci][mover_pt][to][cap_pt];
    // Gravity formula for capture history
    ctx.cap_history[ci][mover_pt][to][cap_pt] =
        old + bonus - (old * bonus.abs()) / HISTORY_GRAVITY;
}

/// Handle beta cutoff: update killers, history, counter moves.
/// `best_score` is the score that beat beta, used to compute is_strong_cutoff.
pub(in crate::search) fn handle_beta_cutoff(
    ctx: &mut SearchContext,
    board: &Board,
    m: Move,
    ply: usize,
    depth: i32,
    is_capture: bool,
    best_score: Score,
    beta: Score,
) {
    // Counterfactual probes are shadow-only: they may observe a full-depth
    // score, but must not train move-ordering heuristics used by the real search.
    if ctx.in_criticality_probe {
        return;
    }
    if is_capture {
        update_cap_history(ctx, board.side, m, board, depth);
        return;
    }
    update_killers(ctx, ply, m);
    let is_strong = best_score > beta + 75;
    let bonus = history_delta(depth, is_strong);
    let moving_piece = board.sq_piece[move_from(m) as usize];
    add_history_score(ctx, board.side, moving_piece, m, bonus);
    // Update continuation history 1-ply
    if ply > 0 && ply < 128 {
        if let Some((pp, pto)) = ctx.stack[ply - 1].cont_entry {
            let pt = piece_type(moving_piece) as usize;
            let to = move_to(m) as usize;
            let old = ctx.cont1[pp][pto][pt][to];
            // Gravity formula for cont history — same GRAVITY, same bonus
            ctx.cont1[pp][pto][pt][to] = old + bonus - (old * bonus.abs()) / HISTORY_GRAVITY;
        }
    }
    // Update continuation history 2-ply with half bonus
    if ply >= 2 && ply < 128 {
        if let Some((pp2, pto2)) = ctx.stack[ply - 2].cont_entry {
            let pt = piece_type(moving_piece) as usize;
            let to = move_to(m) as usize;
            let old = ctx.cont2[pp2][pto2][pt][to];
            let half_bonus = bonus / 2;
            ctx.cont2[pp2][pto2][pt][to] = old + half_bonus - (old * half_bonus.abs()) / HISTORY_GRAVITY;
        }
    }
    // Update continuation history 4-ply with quarter bonus
    if ply >= 4 && ply < 128 {
        if let Some((pp4, pto4)) = ctx.stack[ply - 4].cont_entry {
            let pt = piece_type(moving_piece) as usize;
            let to = move_to(m) as usize;
            let old = ctx.cont4[pp4][pto4][pt][to];
            let quarter_bonus = bonus / 4;
            ctx.cont4[pp4][pto4][pt][to] = old + quarter_bonus - (old * quarter_bonus.abs()) / HISTORY_GRAVITY;
        }
    }
    // Update continuation history 6-ply with quarter bonus
    if ply >= 6 && ply < 128 {
        if let Some((pp6, pto6)) = ctx.stack[ply - 6].cont_entry {
            let pt = piece_type(moving_piece) as usize;
            let to = move_to(m) as usize;
            let old = ctx.cont6[pp6][pto6][pt][to];
            let quarter_bonus = bonus / 4;
            ctx.cont6[pp6][pto6][pt][to] = old + quarter_bonus - (old * quarter_bonus.abs()) / HISTORY_GRAVITY;
        }
    }
    if ply == 0 || ply >= 128 {
        return;
    }
    let prev_move = ctx.stack[ply - 1].current_move;
    if prev_move != MOVE_NONE {
        ctx.counter[move_from(prev_move) as usize][move_to(prev_move) as usize] = m;
    }
}
