use super::test_utils::*;
use super::*;
use std::sync::atomic::AtomicBool;
#[test]
pub(in crate::search) fn node_limit_is_checked_without_4096_node_granularity() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(1);
    let stop = AtomicBool::new(false);
    let limits = Limits {
        max_depth: 8,
        nodes: 1,
        ..Limits::default()
    };
    let mut ctx = test_context(&atk, &z, &mut tt, limits, &stop);
    let mut board = Board::startpos();

    let result = search(&mut board, &mut ctx);

    assert_eq!(result.nodes, 1);
    assert_eq!(result.depth, 0);
    assert!(ctx.stopped);
    assert_ne!(result.best_move, MOVE_NONE);
}
