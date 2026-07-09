use super::test_utils::*;
use super::*;
use std::sync::atomic::AtomicBool;
#[test]
pub(in crate::search) fn quiet_history_updates_the_moving_side() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(1);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let mut board = Board::startpos();

    let white_move = generated_move(&board, &atk, "e2e4");
    handle_beta_cutoff(&mut ctx, &board, white_move, 1, 6, false, 100, 50);
    let white_pt = piece_type(board.sq_piece[move_from(white_move) as usize]) as usize;
    assert!(ctx.history[Color::White as usize][white_pt][move_to(white_move) as usize] > 0);
    assert_eq!(
        ctx.history[Color::Black as usize][white_pt][move_to(white_move) as usize],
        -5 // initial value for un-updated side
    );

    let undo = board.make_move(white_move, &z);
    assert_eq!(board.side, Color::Black);
    let black_move = generated_move(&board, &atk, "e7e5");
    let black_pt = piece_type(board.sq_piece[move_from(black_move) as usize]) as usize;
    handle_beta_cutoff(&mut ctx, &board, black_move, 1, 6, false, 100, 50);
    assert!(ctx.history[Color::Black as usize][black_pt][move_to(black_move) as usize] > 0);
    assert_eq!(
        ctx.history[Color::White as usize][black_pt][move_to(black_move) as usize],
        -5 // initial value for un-updated side
    );
    board.unmake_move(white_move, &undo, &z);
}

#[test]
pub(in crate::search) fn lmr_history_lookup_after_make_move_uses_the_mover_side() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(1);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let mut board = Board::startpos();
    let m = generated_move(&board, &atk, "e2e4");
    let moving_piece = board.sq_piece[move_from(m) as usize];

    add_history_score(&mut ctx, Color::White, moving_piece, m, 1234);
    let undo = board.make_move(m, &z);
    assert_eq!(board.side, Color::Black);

    let mover = board.side.flip() as usize;
    let history_score = ctx.history[mover][piece_type(moving_piece) as usize][move_to(m) as usize];
    assert_eq!(mover, Color::White as usize);
    assert_eq!(history_score, 1229); // init -5 + 1234 = 1229 (gravity term ~0)

    board.unmake_move(m, &undo, &z);
}

#[test]
pub(in crate::search) fn improving_compares_static_eval_two_plies_back() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(1);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let mut board = Board::startpos();

    let eval0 = evaluate(
        &board,
        &EvalContext {
            atk: &atk,
            options: &ctx.options,
            pawn_cache: &ctx.pawn_cache,
        },
    );
    ctx.stack[0].static_eval = Some(eval0);
    assert!(!is_improving(&ctx, eval0, 0));
    assert!(!is_improving(&ctx, -eval0, 1));

    let null_undo = board.make_null_move(&z);
    let null_eval = evaluate(
        &board,
        &EvalContext {
            atk: &atk,
            options: &ctx.options,
            pawn_cache: &ctx.pawn_cache,
        },
    );
    assert_eq!(board.side, Color::Black);
    assert!(!is_improving(&ctx, null_eval, 1));
    ctx.stack[1].static_eval = Some(null_eval);
    board.unmake_null_move(&null_undo);

    let white_move = generated_move(&board, &atk, "e2e4");
    let undo_white = board.make_move(white_move, &z);
    let black_move = generated_move(&board, &atk, "e7e5");
    let undo_black = board.make_move(black_move, &z);
    assert_eq!(board.side, Color::White);

    let eval2 = evaluate(
        &board,
        &EvalContext {
            atk: &atk,
            options: &ctx.options,
            pawn_cache: &ctx.pawn_cache,
        },
    );
    assert_eq!(is_improving(&ctx, eval2, 2), eval2 > eval0);

    board.unmake_move(black_move, &undo_black, &z);
    board.unmake_move(white_move, &undo_white, &z);
}

#[test]
pub(in crate::search) fn quiet_history_distribution_is_not_immediately_saturated() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(
        &atk,
        &z,
        &mut tt,
        Limits {
            max_depth: 6,
            ..Limits::default()
        },
        &stop,
    );
    let mut board = Board::startpos();
    ctx.root_color = board.side;
    let mut pv = Vec::new();
    let _ = alpha_beta(
        &mut board,
        &mut ctx,
        SearchNode {
            alpha: -SCORE_INF,
            beta: SCORE_INF,
            depth: 6,
            ply: 0,
            is_pv: true,
            excluded_move: None,
        },
        &mut pv,
    );

    let mut white_abs_sum = 0i64;
    let mut black_abs_sum = 0i64;
    let mut max_abs = 0i32;
    let mut nonzero = 0usize;
    for pt in 0..6 {
        for to in 0..64 {
            let white = ctx.history[Color::White as usize][pt][to].abs();
            let black = ctx.history[Color::Black as usize][pt][to].abs();
            white_abs_sum += white as i64;
            black_abs_sum += black as i64;
            max_abs = max_abs.max(white).max(black);
            nonzero += usize::from(white != 0) + usize::from(black != 0);
        }
    }

    eprintln!(
        "history distribution: white_abs_sum={white_abs_sum} black_abs_sum={black_abs_sum} max_abs={max_abs} nonzero={nonzero}"
    );

    assert!(nonzero > 0);
    assert!(max_abs <= HISTORY_GRAVITY);
    assert!(white_abs_sum > 0);
    assert!(black_abs_sum > 0);
}

#[test]
pub(in crate::search) fn history_delta_obsidian_formula() {
    // Obsidian formula: (175 * d + 15).min(1409)
    assert_eq!(history_delta(1, false), 175 * 1 + 15); // 190
    assert_eq!(history_delta(1, true), 175 * 2 + 15); // 365 (strong cutoff adds 1 to depth)
    assert_eq!(history_delta(2, false), 175 * 2 + 15); // 365
    assert_eq!(history_delta(2, true), 175 * 3 + 15); // 540
    assert_eq!(history_delta(5, false), 175 * 5 + 15); // 890
    assert_eq!(history_delta(5, true), 175 * 6 + 15); // 1065
                                                      // Cap at 1409
    assert_eq!(history_delta(8, true), 1409); // 175*9+15 = 1590, capped
    assert_eq!(history_delta(10, true), 1409); // well above cap
}

#[test]
pub(in crate::search) fn history_malus_obsidian_formula() {
    // Obsidian malus: -(196 * depth - 25).min(1047).max(-1047)
    assert_eq!(history_malus(1), -(196 * 1 - 25)); // -171
    assert_eq!(history_malus(2), -(196 * 2 - 25)); // -367
    assert_eq!(history_malus(5), -(196 * 5 - 25)); // -955
    assert_eq!(history_malus(6), -1047); // -(196*6-25) = -1151, clamped to -1047
    assert_eq!(history_malus(10), -1047); // well below clamp
}

#[test]
pub(in crate::search) fn cont_history_1ply_updates_on_beta_cutoff() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let mut board = Board::startpos();

    // Make a move at ply 0 and set cont_entry for the child
    let white_move = generated_move(&board, &atk, "e2e4");
    let white_piece = board.sq_piece[move_from(white_move) as usize];
    let white_pt = piece_type(white_piece) as usize;
    let white_to = move_to(white_move) as usize;
    ctx.stack[0].cont_entry = Some((white_pt, white_to));

    let undo = board.make_move(white_move, &z);
    // Now at ply 1 (Black's turn)
    let black_move = generated_move(&board, &atk, "e7e5");
    let black_piece = board.sq_piece[move_from(black_move) as usize];
    let black_pt = piece_type(black_piece) as usize;
    let black_to = move_to(black_move) as usize;

    // Simulate beta cutoff at ply 1 — should update cont1[white_pt][white_to][black_pt][black_to]
    handle_beta_cutoff(
        &mut ctx, &board, black_move, 1, 6, false, 100, 50, // best_score=100, beta=50
    );

    let entry = ctx.cont1[white_pt][white_to][black_pt][black_to];
    assert!(
        entry > -552,
        "cont history should increase from -552 on beta cutoff, got {entry}"
    );

    board.unmake_move(white_move, &undo, &z);
}

#[test]
pub(in crate::search) fn cont_history_initialized_to_negative_bias() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);

    assert_eq!(ctx.cont1[0][0][0][0], -552);
    assert_eq!(ctx.cont1[3][40][1][20], -552);
}

#[test]
pub(in crate::search) fn cont_history_2ply_updates_with_half_bonus() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let mut board = Board::startpos();

    // Set cont_entry at ply 0 — the 2-ply update at ply 2 reads stack[ply-2] = stack[0]
    ctx.stack[0].cont_entry = Some((PieceType::Pawn as usize, 28)); // e2-e4

    let wm = generated_move(&board, &atk, "e2e4");
    let undo0 = board.make_move(wm, &z);
    // ply 1: Black's turn
    let bm = generated_move(&board, &atk, "e7e5");
    let undo1 = board.make_move(bm, &z);

    // Now at ply 2 (White's turn), cause a beta cutoff
    let wm2 = generated_move(&board, &atk, "g1f3");
    let wm2_pt = piece_type(board.sq_piece[move_from(wm2) as usize]) as usize;
    let wm2_to = move_to(wm2) as usize;

    handle_beta_cutoff(&mut ctx, &board, wm2, 2, 6, false, 100, 50);

    // cont2 should be updated: cont2[Pawn][28][Knight][f3]
    // (reads stack[ply-2].cont_entry = stack[0].cont_entry)
    let pp = PieceType::Pawn as usize;
    let entry = ctx.cont2[pp][28][wm2_pt][wm2_to];
    assert!(
        entry > -552,
        "cont2 should increase from -552 on beta cutoff, got {entry}"
    );

    board.unmake_move(bm, &undo1, &z);
    board.unmake_move(wm, &undo0, &z);
}

#[test]
pub(in crate::search) fn cont_history_4ply_and_6ply_tables_exist() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);

    // All tables initialized to -552
    assert_eq!(ctx.cont4[0][0][0][0], -552);
    assert_eq!(ctx.cont6[0][0][0][0], -552);
}

#[test]
pub(in crate::search) fn pawn_history_updates_on_beta_cutoff() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let board = Board::startpos();

    let white_move = generated_move(&board, &atk, "e2e4");
    let w_pt = piece_type(board.sq_piece[move_from(white_move) as usize]) as usize;
    let w_to = move_to(white_move) as usize;

    let pawn_idx = (board.pawn_hash & 1023) as usize;
    assert_eq!(ctx.pawn_history[pawn_idx][w_pt][w_to], -5);

    handle_beta_cutoff(&mut ctx, &board, white_move, 1, 6, false, 100, 50);

    assert!(
        ctx.pawn_history[pawn_idx][w_pt][w_to] > -5,
        "pawn history should increase on beta cutoff"
    );
}

#[test]
pub(in crate::search) fn pawn_hash_changes_after_pawn_move() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut board = Board::startpos();
    let hash_before = board.pawn_hash;

    let wm = generated_move(&board, &atk, "e2e4");
    let undo = board.make_move(wm, &z);
    assert_ne!(
        board.pawn_hash, hash_before,
        "pawn hash should change after a pawn move"
    );

    board.unmake_move(wm, &undo, &z);
    assert_eq!(
        board.pawn_hash, hash_before,
        "pawn hash should be restored after unmake"
    );
}

#[test]
pub(in crate::search) fn correction_history_is_zero_on_startpos() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let board = Board::startpos();

    // At the start position, correction should be zero (no history yet)
    let corr = compute_correction(&ctx, &board, 0);
    assert_eq!(corr, 0, "correction should be zero with no history");
}

#[test]
pub(in crate::search) fn correction_history_updates_after_search() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let board = Board::startpos();

    // Simulate a search returning a score that differs from raw eval
    let raw_eval = 30;
    let best_score = 70;

    update_correction(&mut ctx, &board, 6, best_score, raw_eval, 1);

    let corr = compute_correction(&ctx, &board, 1);
    assert!(
        corr > 0,
        "correction should be positive after positive diff, got {corr}"
    );
}

#[test]
pub(in crate::search) fn correction_history_components_exist() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);

    assert_eq!(ctx.pawn_corr[0][0], 0);
    assert_eq!(ctx.nonpawn_corr_w[0][0], 0);
    assert_eq!(ctx.nonpawn_corr_b[0][0], 0);
    assert_eq!(ctx.cont_corr[0][0][0], 0);
}
