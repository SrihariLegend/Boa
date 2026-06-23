use super::*;
use crate::tt::TranspositionTable;
use std::sync::atomic::AtomicBool;

pub(in crate::search) fn test_context<'a>(
    atk: &'a AttackTables,
    z: &'a Zobrist,
    tt: &'a mut TranspositionTable,
    limits: Limits,
    stop: &'a AtomicBool,
) -> SearchContext<'a> {
    SearchContext::new(
        atk,
        z,
        tt,
        limits,
        Vec::new(),
        0,
        EngineOptions::default(),
        None,
        stop,
        0,
        0,
    )
}

pub(in crate::search) fn generated_move(board: &Board, atk: &AttackTables, uci: &str) -> Move {
    let list = gen_moves(board, atk);
    for i in 0..list.count {
        if move_name(list.moves[i]) == uci {
            return list.moves[i];
        }
    }
    panic!("move {uci} was not generated in {}", board.to_fen());
}

pub(in crate::search) fn see_for(fen: &str, uci: &str) -> i32 {
    let atk = AttackTables::init();
    let board = Board::from_fen(fen).unwrap();
    let m = generated_move(&board, &atk, uci);
    static_exchange_eval(&board, &atk, m)
}

pub(in crate::search) fn lmr_reduction_details_for(input: LmrInput) -> LmrReduction {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(1);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    compute_lmr_reduction_details(input, &mut ctx)
}

pub(in crate::search) fn lmr_reduction_for(input: LmrInput) -> i32 {
    lmr_reduction_details_for(input).final_reduction
}

pub(in crate::search) fn reducible_lmr_input(depth: i32, moves_searched: usize) -> LmrInput {
    LmrInput {
        moves_searched,
        move_index: moves_searched,
        ply: 0,
        depth,
        history_score: 0,
        static_eval: 0,
        prev_static_eval: None,
        alpha: 0,
        beta: 0,
        root_depth: depth,
        side_to_move: Color::White,
        moving_piece: make_piece(Color::White, PieceType::Pawn),
        is_pv: false,
        is_cut_node: false,
        improving: false,
        is_killer: false,
        is_counter: false,
        tt_move_agreement: false,
        is_capture: false,
        is_promo: false,
        gives_check: false,
        in_check: false,
    }
}
