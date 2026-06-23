use super::*;
pub(super) fn handle_position<'a>(
    mut tokens: impl Iterator<Item = &'a str>,
    board: &mut Board,
    position_history: &mut Vec<u64>,
    atk: &AttackTables,
    z: &Zobrist,
) {
    position_history.clear();
    match tokens.next() {
        Some("startpos") => {
            *board = Board::startpos();
        }
        Some("fen") => {
            let fen = collect_fen(&mut tokens);
            *board = Board::from_fen(&fen).unwrap_or_else(Board::startpos);
        }
        _ => {}
    }
    position_history.push(board.hash);
    for tok in tokens {
        let Some(m) = move_from_uci(tok) else {
            continue;
        };
        let Some(lm) = find_legal_move(board, atk, z, m) else {
            continue;
        };
        let _undo = board.make_move(lm, z);
        position_history.push(board.hash);
    }
}

pub(super) fn collect_fen<'a>(tokens: &mut impl Iterator<Item = &'a str>) -> String {
    let mut fen_parts = Vec::new();
    for tok in tokens {
        if tok == "moves" {
            break;
        }
        fen_parts.push(tok);
    }
    fen_parts.join(" ")
}
