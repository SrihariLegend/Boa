use super::*;
pub(super) fn is_legal_move(board: &Board, z: &Zobrist, lm: Move) -> bool {
    let mut b = board.clone();
    let _undo = b.make_move(lm, z);
    !b.is_in_check(b.side.flip())
}

pub(super) fn find_legal_move(
    board: &Board,
    atk: &AttackTables,
    z: &Zobrist,
    m: Move,
) -> Option<Move> {
    use crate::movegen::gen_moves;
    let from = move_from(m);
    let to = move_to(m);
    let promo_flag = move_flags(m) == MF_PROMOTION;

    let list = gen_moves(board, atk);
    for &lm in list.iter() {
        if move_from(lm) != from || move_to(lm) != to {
            continue;
        }
        if promo_flag && (move_flags(lm) != MF_PROMOTION || move_promo_pt(lm) != move_promo_pt(m)) {
            continue;
        }
        if is_legal_move(board, z, lm) {
            return Some(lm);
        }
    }
    None
}
