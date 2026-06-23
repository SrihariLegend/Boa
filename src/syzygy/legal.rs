use super::*;
pub(super) fn find_legal_move(
    board: &Board,
    atk: &AttackTables,
    z: &Zobrist,
    m: Move,
) -> Option<Move> {
    let from = move_from(m);
    let to = move_to(m);
    let promo_flag = move_flags(m) == MF_PROMOTION;
    let list = gen_moves(board, atk);

    for &legal_move in list.iter() {
        if move_from(legal_move) != from || move_to(legal_move) != to {
            continue;
        }
        if promo_flag
            && (move_flags(legal_move) != MF_PROMOTION
                || move_promo_pt(legal_move) != move_promo_pt(m))
        {
            continue;
        }

        let mut clone = board.clone();
        let undo = clone.make_move(legal_move, z);
        let legal = !clone.is_in_check(clone.side.flip());
        clone.unmake_move(legal_move, &undo, z);
        if legal {
            return Some(legal_move);
        }
    }

    None
}
