use super::*;
use crate::probe;
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

    // Board probe — one per position command
    let fen = board.to_fen();
    probe!(
        Board,
        BoardEvent {
            fen: if fen.len() > 64 {
                fen[..64].to_string()
            } else {
                fen
            },
            phase: 0,
            non_pawn_material: board.non_pawn_material(Color::White)
                + board.non_pawn_material(Color::Black),
            mobile_pieces: 0,
            open_files: 0,
            in_check: board.is_in_check(board.side),
            material_rule_score: 0,
            halfmove_clock: board.halfmove as i32,
            fullmove_number: board.fullmove as i32,
        }
    );

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
