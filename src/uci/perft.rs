use super::*;
pub(super) fn run_perft(board: &mut Board, atk: &AttackTables, z: &Zobrist, depth: u32) {
    use std::time::Instant;
    let start = Instant::now();
    if depth == 0 {
        println!("perft(0) = 1 nodes in 0ms");
        return;
    }
    let list = crate::movegen::gen_moves(board, atk);
    let mut total = 0u64;
    for i in 0..list.count {
        let m = list.moves[i];
        let undo = board.make_move(m, z);
        if board.is_in_check(board.side.flip()) {
            board.unmake_move(m, &undo, z);
            continue;
        }
        let n = perft(board, atk, z, depth - 1);
        board.unmake_move(m, &undo, z);
        if depth >= 2 {
            println!("{}: {}", move_name(m), n);
        }
        total += n;
    }
    let elapsed = start.elapsed().as_millis().max(1) as u64;
    println!(
        "perft({}) = {} nodes in {}ms ({} nps)",
        depth,
        total,
        elapsed,
        total * 1000 / elapsed
    );
}
