use super::*;
// ============================================================
// Section 4b: Perft (for correctness verification)
// ============================================================

pub fn perft(board: &mut Board, atk: &AttackTables, z: &crate::board::Zobrist, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let list = gen_moves(board, atk);
    let mut nodes = 0u64;
    for i in 0..list.count {
        let m = list.moves[i];
        let undo = board.make_move(m, z);
        if !board.is_in_check(board.side.flip()) {
            nodes += perft(board, atk, z, depth - 1);
        }
        board.unmake_move(m, &undo, z);
    }
    nodes
}
