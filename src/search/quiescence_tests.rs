use super::test_utils::*;
use super::*;
use std::sync::atomic::AtomicBool;
#[test]
pub(in crate::search) fn quiescence_reports_checkmate_instead_of_standing_pat() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(1);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let mut board = Board::from_fen("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1").unwrap();

    let score = quiescence(&mut board, &mut ctx, -SCORE_INF, SCORE_INF, 0, 0);
    assert_eq!(score, -SCORE_MATE);
}

#[test]
pub(in crate::search) fn depth_zero_in_check_detects_mate() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(1);
    let stop = AtomicBool::new(false);
    let mut board = Board::from_fen("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1").unwrap();
    let mut ctx = test_context(
        &atk,
        &z,
        &mut tt,
        Limits {
            max_depth: 1,
            ..Limits::default()
        },
        &stop,
    );
    ctx.root_color = board.side;

    let mut pv = Vec::new();
    let score = alpha_beta(
        &mut board,
        &mut ctx,
        SearchNode {
            alpha: -SCORE_INF,
            beta: SCORE_INF,
            depth: 0,
            ply: 0,
            is_pv: true,
        },
        &mut pv,
    );

    assert_eq!(score, -SCORE_MATE);
}
