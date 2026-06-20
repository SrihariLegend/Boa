// ============================================================
// diagnostics.rs - offline feature extraction for engine research
// ============================================================

use crate::board::{Board, Zobrist};
use crate::config::EngineOptions;
use crate::eval::{evaluate_breakdown, side_mobility, EvalContext};
use crate::movegen::{gen_moves, AttackTables};
use crate::types::*;

const LIBERATING_MOBILITY_GAIN: u32 = 5;
const REDEPLOYMENT_MOBILITY_GAIN: u32 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestrictionFeatures {
    pub fen: String,
    pub side_to_move: Color,
    pub static_eval_cp: i32,
    pub white_score_cp: i32,
    pub phase: i32,
    pub material_cp: i32,
    pub pst_cp: i32,
    pub mobility_cp: i32,
    pub pawn_structure_cp: i32,
    pub king_safety_cp: i32,
    pub freedom_cp: i32,
    pub trade_down_cp: i32,
    pub weak_squares_cp: i32,
    pub coordination_cp: i32,
    pub advanced_pawns_cp: i32,
    pub material_mg: i32,
    pub material_eg: i32,
    pub pst_mg: i32,
    pub pst_eg: i32,
    pub mobility_white: u32,
    pub mobility_black: u32,
    pub mobility_mg: i32,
    pub mobility_eg: i32,
    pub pawn_structure_mg: i32,
    pub pawn_structure_eg: i32,
    pub king_safety_mg: i32,
    pub king_safety_eg: i32,
    pub freedom: i32,
    pub trade_down_mg: i32,
    pub trade_down_eg: i32,
    pub weak_squares_mg: i32,
    pub weak_squares_eg: i32,
    pub coordination_mg: i32,
    pub coordination_eg: i32,
    pub advanced_pawns_mg: i32,
    pub advanced_pawns_eg: i32,
    pub white_pawn_breaks: u32,
    pub black_pawn_breaks: u32,
    pub liberating_breaks_white: u32,
    pub liberating_breaks_black: u32,
    pub piece_redeployment_white: u32,
    pub piece_redeployment_black: u32,
}

impl RestrictionFeatures {
    pub fn csv_header() -> &'static str {
        "fen,side_to_move,static_eval_cp,white_score_cp,phase,\
         material_cp,pst_cp,mobility_cp,pawn_structure_cp,king_safety_cp,freedom_cp,\
         trade_down_cp,weak_squares_cp,coordination_cp,advanced_pawns_cp,\
         material_mg,material_eg,pst_mg,pst_eg,\
         mobility_white,mobility_black,mobility_mg,mobility_eg,\
         pawn_structure_mg,pawn_structure_eg,king_safety_mg,king_safety_eg,freedom,\
         trade_down_mg,trade_down_eg,weak_squares_mg,weak_squares_eg,\
         coordination_mg,coordination_eg,advanced_pawns_mg,advanced_pawns_eg,\
         white_pawn_breaks,black_pawn_breaks,liberating_breaks_white,liberating_breaks_black,\
         piece_redeployment_white,piece_redeployment_black"
    }

    pub fn to_csv_row(&self) -> String {
        [
            csv_string(&self.fen),
            color_name(self.side_to_move).to_string(),
            self.static_eval_cp.to_string(),
            self.white_score_cp.to_string(),
            self.phase.to_string(),
            self.material_cp.to_string(),
            self.pst_cp.to_string(),
            self.mobility_cp.to_string(),
            self.pawn_structure_cp.to_string(),
            self.king_safety_cp.to_string(),
            self.freedom_cp.to_string(),
            self.trade_down_cp.to_string(),
            self.weak_squares_cp.to_string(),
            self.coordination_cp.to_string(),
            self.advanced_pawns_cp.to_string(),
            self.material_mg.to_string(),
            self.material_eg.to_string(),
            self.pst_mg.to_string(),
            self.pst_eg.to_string(),
            self.mobility_white.to_string(),
            self.mobility_black.to_string(),
            self.mobility_mg.to_string(),
            self.mobility_eg.to_string(),
            self.pawn_structure_mg.to_string(),
            self.pawn_structure_eg.to_string(),
            self.king_safety_mg.to_string(),
            self.king_safety_eg.to_string(),
            self.freedom.to_string(),
            self.trade_down_mg.to_string(),
            self.trade_down_eg.to_string(),
            self.weak_squares_mg.to_string(),
            self.weak_squares_eg.to_string(),
            self.coordination_mg.to_string(),
            self.coordination_eg.to_string(),
            self.advanced_pawns_mg.to_string(),
            self.advanced_pawns_eg.to_string(),
            self.white_pawn_breaks.to_string(),
            self.black_pawn_breaks.to_string(),
            self.liberating_breaks_white.to_string(),
            self.liberating_breaks_black.to_string(),
            self.piece_redeployment_white.to_string(),
            self.piece_redeployment_black.to_string(),
        ]
        .join(",")
    }
}

pub fn extract_restriction_features(
    board: &Board,
    atk: &AttackTables,
    z: &Zobrist,
    options: EngineOptions,
) -> RestrictionFeatures {
    let ctx = EvalContext {
        atk,
        options: &options,
    };
    let eval = evaluate_breakdown(board, &ctx);
    let white_breaks = count_pawn_breaks(board, atk, z, Color::White);
    let black_breaks = count_pawn_breaks(board, atk, z, Color::Black);

    RestrictionFeatures {
        fen: board.to_fen(),
        side_to_move: board.side,
        static_eval_cp: eval.side_to_move_score,
        white_score_cp: eval.white_score,
        phase: eval.phase,
        material_cp: eval.material_cp,
        pst_cp: eval.pst_cp,
        mobility_cp: eval.mobility_cp,
        pawn_structure_cp: eval.pawn_structure_cp,
        king_safety_cp: eval.king_safety_cp,
        freedom_cp: eval.freedom,
        trade_down_cp: eval.trade_down_cp,
        weak_squares_cp: eval.weak_squares_cp,
        coordination_cp: eval.coordination_cp,
        advanced_pawns_cp: eval.advanced_pawns_cp,
        material_mg: eval.material_mg,
        material_eg: eval.material_eg,
        pst_mg: eval.pst_mg,
        pst_eg: eval.pst_eg,
        mobility_white: eval.mobility_white,
        mobility_black: eval.mobility_black,
        mobility_mg: eval.mobility_mg,
        mobility_eg: eval.mobility_eg,
        pawn_structure_mg: eval.pawn_structure_mg,
        pawn_structure_eg: eval.pawn_structure_eg,
        king_safety_mg: eval.king_safety_mg,
        king_safety_eg: eval.king_safety_eg,
        freedom: eval.freedom,
        trade_down_mg: eval.trade_down_mg,
        trade_down_eg: eval.trade_down_eg,
        weak_squares_mg: eval.weak_squares_mg,
        weak_squares_eg: eval.weak_squares_eg,
        coordination_mg: eval.coordination_mg,
        coordination_eg: eval.coordination_eg,
        advanced_pawns_mg: eval.advanced_pawns_mg,
        advanced_pawns_eg: eval.advanced_pawns_eg,
        white_pawn_breaks: white_breaks.total,
        black_pawn_breaks: black_breaks.total,
        liberating_breaks_white: white_breaks.liberating,
        liberating_breaks_black: black_breaks.liberating,
        piece_redeployment_white: count_piece_redeployments(board, atk, z, Color::White),
        piece_redeployment_black: count_piece_redeployments(board, atk, z, Color::Black),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PawnBreakCounts {
    total: u32,
    liberating: u32,
}

fn count_pawn_breaks(
    board: &Board,
    atk: &AttackTables,
    z: &Zobrist,
    color: Color,
) -> PawnBreakCounts {
    let before_mobility = mobility_for(board, atk, color);
    let mut counts = PawnBreakCounts::default();

    for m in legal_moves_for_color(board, atk, z, color) {
        if !is_pawn_break(board, color, m) {
            continue;
        }
        counts.total += 1;

        let mut next = board.clone();
        prepare_side_to_move(&mut next, color);
        let undo = next.make_move(m, z);
        let after_mobility = mobility_for(&next, atk, color);
        next.unmake_move(m, &undo, z);

        if after_mobility > before_mobility + LIBERATING_MOBILITY_GAIN {
            counts.liberating += 1;
        }
    }

    counts
}

fn is_pawn_break(board: &Board, color: Color, m: Move) -> bool {
    let from = move_from(m);
    let to = move_to(m);
    let mover = board.sq_piece[from as usize];
    if mover == PIECE_NONE || piece_color(mover) != color || piece_type(mover) != PieceType::Pawn {
        return false;
    }

    let captures = board.sq_piece[to as usize] != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
    let opens_source_file = captures && source_file_is_clear_after_move(board, color, from);
    let creates_passer = !is_passed_pawn(board, color, from) && {
        let mut next = board.clone();
        prepare_side_to_move(&mut next, color);
        let z = Zobrist::new();
        let undo = next.make_move(m, &z);
        let passed = move_flags(m) != MF_PROMOTION && is_passed_pawn(&next, color, to);
        next.unmake_move(m, &undo, &z);
        passed
    };

    opens_source_file || creates_passer
}

fn source_file_is_clear_after_move(board: &Board, color: Color, from: Square) -> bool {
    let pawns = board.pieces[color as usize][PieceType::Pawn as usize];
    let file = BB_FILES[sq_file(from) as usize];
    pawns & file & !bb(from) == 0
}

fn is_passed_pawn(board: &Board, color: Color, sq: Square) -> bool {
    let file = sq_file(sq);
    let rank = sq_rank(sq);
    let mut files = BB_FILES[file as usize];
    if file > 0 {
        files |= BB_FILES[(file - 1) as usize];
    }
    if file < 7 {
        files |= BB_FILES[(file + 1) as usize];
    }

    let their_pawns = board.pieces[color.flip() as usize][PieceType::Pawn as usize];
    their_pawns & ranks_ahead(color, rank, files) == 0
}

fn ranks_ahead(color: Color, rank: u8, files: Bb) -> Bb {
    let mut ranks = 0u64;
    if color == Color::White {
        for r in (rank + 1)..8 {
            ranks |= BB_RANKS[r as usize];
        }
    } else {
        for r in 0..rank {
            ranks |= BB_RANKS[r as usize];
        }
    }
    ranks & files
}

fn count_piece_redeployments(board: &Board, atk: &AttackTables, z: &Zobrist, color: Color) -> u32 {
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

fn is_quiet_piece_move(board: &Board, color: Color, m: Move) -> bool {
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

fn piece_mobility(
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

fn legal_moves_for_color(
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

fn mobility_for(board: &Board, atk: &AttackTables, color: Color) -> u32 {
    let ctx = EvalContext {
        atk,
        options: &EngineOptions::default(),
    };
    side_mobility(board, &ctx, color)
}

fn prepare_side_to_move(board: &mut Board, color: Color) {
    if board.side != color {
        board.side = color;
        board.ep_sq = NO_SQUARE;
    }
}

fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "w",
        Color::Black => "b",
    }
}

fn csv_string(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> (AttackTables, Zobrist) {
        (AttackTables::init(), Zobrist::new())
    }

    #[test]
    fn startpos_features_are_symmetric_for_raw_mobility() {
        let (atk, z) = test_context();
        let board = Board::startpos();
        let features = extract_restriction_features(&board, &atk, &z, EngineOptions::default());

        assert_eq!(features.mobility_white, 20);
        assert_eq!(features.mobility_black, 20);
        assert_eq!(features.white_pawn_breaks, features.black_pawn_breaks);
        assert_eq!(
            features.piece_redeployment_white,
            features.piece_redeployment_black
        );
    }

    #[test]
    fn passed_pawn_push_counts_as_pawn_break() {
        let (atk, z) = test_context();
        let board = Board::from_fen("4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1").unwrap();
        let features = extract_restriction_features(&board, &atk, &z, EngineOptions::default());

        assert!(features.white_pawn_breaks > 0);
    }

    #[test]
    fn csv_row_matches_header_width() {
        let (atk, z) = test_context();
        let board = Board::startpos();
        let features = extract_restriction_features(&board, &atk, &z, EngineOptions::default());

        assert_eq!(
            RestrictionFeatures::csv_header().split(',').count(),
            features.to_csv_row().split(',').count()
        );
    }
}
