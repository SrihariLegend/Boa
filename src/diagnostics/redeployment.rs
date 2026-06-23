use super::*;
pub(in crate::diagnostics) fn count_piece_redeployments(
    board: &Board,
    atk: &AttackTables,
    z: &Zobrist,
    color: Color,
) -> u32 {
    let mut count = 0u32;
    for m in legal_moves_for_color(board, atk, z, color) {
        if !is_quiet_piece_move(board, color, m) {
            continue;
        }

        let from = move_from(m);
        let to = move_to(m);
        let pt = piece_type(board.sq_piece[from as usize]);
        let before = piece_mobility(board, atk, color, pt, from);

        let mut next = board.clone();
        prepare_side_to_move(&mut next, color);
        let undo = next.make_move(m, z);
        let after = piece_mobility(&next, atk, color, pt, to);
        next.unmake_move(m, &undo, z);

        if after >= before + REDEPLOYMENT_MOBILITY_GAIN {
            count += 1;
        }
    }
    count
}

pub(in crate::diagnostics) fn is_quiet_piece_move(board: &Board, color: Color, m: Move) -> bool {
    let from = move_from(m);
    let to = move_to(m);
    let mover = board.sq_piece[from as usize];
    if mover == PIECE_NONE || piece_color(mover) != color {
        return false;
    }
    let pt = piece_type(mover);
    if pt == PieceType::Pawn || pt == PieceType::King {
        return false;
    }
    move_flags(m) == MF_NORMAL && board.sq_piece[to as usize] == PIECE_NONE
}

pub(in crate::diagnostics) fn piece_mobility(
    board: &Board,
    atk: &AttackTables,
    color: Color,
    pt: PieceType,
    sq: Square,
) -> u32 {
    let our_occ = board.occ[color as usize];
    let attacks = match pt {
        PieceType::Knight => atk.knight[sq as usize],
        PieceType::Bishop => atk.bishop_attacks(sq, board.occ_all),
        PieceType::Rook => atk.rook_attacks(sq, board.occ_all),
        PieceType::Queen => atk.queen_attacks(sq, board.occ_all),
        _ => 0,
    };
    (attacks & !our_occ).count_ones()
}

pub(in crate::diagnostics) fn legal_moves_for_color(
    board: &Board,
    atk: &AttackTables,
    z: &Zobrist,
    color: Color,
) -> Vec<Move> {
    let mut b = board.clone();
    prepare_side_to_move(&mut b, color);
    let list = gen_moves(&b, atk);
    let mut legal = Vec::new();

    for &m in list.iter() {
        let undo = b.make_move(m, z);
        if !b.is_in_check(b.side.flip()) {
            legal.push(m);
        }
        b.unmake_move(m, &undo, z);
    }

    legal
}

pub(in crate::diagnostics) fn mobility_for(board: &Board, atk: &AttackTables, color: Color) -> u32 {
    let ctx = EvalContext {
        atk,
        options: &EngineOptions::default(),
    };
    side_mobility(board, &ctx, color)
}

pub(in crate::diagnostics) fn prepare_side_to_move(board: &mut Board, color: Color) {
    if board.side != color {
        board.side = color;
        board.ep_sq = NO_SQUARE;
    }
}

pub(in crate::diagnostics) fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "w",
        Color::Black => "b",
    }
}

pub(in crate::diagnostics) fn csv_string(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
