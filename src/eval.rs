// ============================================================
// eval.rs - Classical tapered evaluation
//
// Score is always from the perspective of the side to move (negamax).
//
// Structure:
//   1. Piece-square tables (midgame + endgame, tapered)
//   2. Pawn structure (passed, isolated, doubled, chains)
//   3. Mobility
//   4. Piece activity (outposts, rooks on open files, bishop pair)
//   5. King safety
//   6. Passed pawn advancement safety (clear path + king support)
// ============================================================

use crate::board::Board;
use crate::config::{scale_score_pair, EngineOptions};
use crate::movegen::{pawn_attacks_black, pawn_attacks_white, AttackTables};
use crate::types::*;

// ---- Evaluation tuning constants ----
// Sources: Stockfish (SF), Chess Programming Wiki (CPW), or marked [NEEDS TUNING].
//
// Piece values are defined in types.rs (Kaufman values: P=100, N=320, B=330, R=500, Q=900).

/// Bishop pair bonus (mg, eg). Having two bishops is worth extra material.
/// SF uses ~30/50 (tuned). CPW recommends 25-50. [NEEDS TUNING]
const BISHOP_PAIR_BONUS: (i32, i32) = (30, 50);

/// Rook on fully open file (no pawns of either color). (mg, eg)
/// SF: ~20/7. CPW: 15-25. [NEEDS TUNING]
const ROOK_OPEN_FILE_BONUS: (i32, i32) = (20, 10);

/// Rook on semi-open file (no friendly pawns). (mg, eg)
/// SF: ~7/6. CPW: 8-15. [NEEDS TUNING]
const ROOK_SEMI_OPEN_FILE_BONUS: (i32, i32) = (10, 5);

/// Rook on 7th rank bonus. Strong in both phases.
/// SF: ~15-30 depending on context. CPW: 20-30. [NEEDS TUNING]
const ROOK_ON_SEVENTH_BONUS: (i32, i32) = (20, 30);

/// Knight outpost bonus when supported/unsupported by own pawn.
/// An outpost is a square on ranks 4-6 not attackable by enemy pawns.
/// SF: ~30-50 for supported outposts. These are conservative. [NEEDS TUNING]
/// (Values were swapped — a supported outpost must outscore an unsupported one.)
const OUTPOST_SUPPORTED: i32 = 20;
const OUTPOST_UNSUPPORTED: i32 = 10;

/// Tempo bonus: side-to-move advantage in centipawns.
/// SF uses ~28 (tuned). 15 is conservative. [NEEDS TUNING]
const TEMPO_BONUS: i32 = 10;

/// Doubled pawn penalty (mg, eg). Two pawns on same file.
/// SF: ~-5/-20 (file-dependent). CPW: -10 to -20. [NEEDS TUNING]
const DOUBLED_PAWN_PENALTY: (i32, i32) = (-5, -10);

/// Isolated pawn penalty (mg, eg). No friendly pawns on adjacent files.
/// SF: ~-10/-20. CPW: -15 to -25. [NEEDS TUNING]
const ISOLATED_PAWN_PENALTY: (i32, i32) = (-10, -20);

/// Backward pawn penalty (mg, eg). Pawn on starting rank with no adjacent support.
/// Less studied than isolated. SF has complex backward pawn logic. [NEEDS TUNING]
const BACKWARD_PAWN_PENALTY: (i32, i32) = (-8, -12);

/// Pawn chain bonus per protected pawn (mg, eg).
/// Pawns defending each other. SF: ~3-5. [NEEDS TUNING]
const PAWN_CHAIN_BONUS: (i32, i32) = (3, 5);

/// Passed pawn bonus tables indexed by advancement (distance from promotion).
/// Values increase exponentially as pawn advances. Shape follows SF/CPW convention. [NEEDS TUNING]
const PASSED_PAWN_BONUS_MG: [i32; 8] = [0, 0, 5, 10, 20, 40, 70, 0];
const PASSED_PAWN_BONUS_EG: [i32; 8] = [0, 5, 10, 20, 40, 80, 120, 0];

/// Pawn shield: bonus per shielding pawn, with a base penalty for exposed king.
/// shield_score = count * PER_PAWN - BASE_PENALTY
/// With 3 shield pawns: 3*10 - 30 = 0 (neutral). 0 pawns: -30. [NEEDS TUNING]
const PAWN_SHIELD_PER_PAWN: i32 = 10;
const PAWN_SHIELD_BASE_PENALTY: i32 = 30;

/// King zone attack unit weights by piece type.
/// Each piece attacking the king zone contributes this many "attack units".
/// Inspired by CPW safety tables. Queens count most, pawns/kings not counted. [NEEDS TUNING]
const KING_ATTACK_WEIGHT_KNIGHT: i32 = 2;
const KING_ATTACK_WEIGHT_BISHOP: i32 = 2;
const KING_ATTACK_WEIGHT_ROOK: i32 = 3;
const KING_ATTACK_WEIGHT_QUEEN: i32 = 5;

/// King safety penalty table: maps attack_units to penalty.
/// Loosely follows the CPW safety table shape (quadratic-ish growth). [NEEDS TUNING]
const KING_SAFETY_TABLE: [(i32, i32); 7] = [
    // (max_attack_units, penalty)
    (2, 0),
    (5, 10),
    (8, 25),
    (11, 50),
    (15, 80),
    (20, 120),
    (i32::MAX, 160),
];

/// Rook behind passed pawn bonus. Rooks belong behind passers (Tarrasch rule).
/// Applies to both own and enemy passed pawns. [NEEDS TUNING]
const ROOK_BEHIND_PASSER_BONUS: (i32, i32) = (10, 20);

/// King centralization in endgame: bonus per rank/file closer to center. [NEEDS TUNING]
const KING_CENTRALIZATION_EG: i32 = 10;

/// Connected passed pawn bonus multiplier.
/// Two passed pawns on adjacent files supporting each other. [NEEDS TUNING]
const CONNECTED_PASSER_BONUS: (i32, i32) = (10, 20);

/// Passed pawn path clear bonus: extra bonus when no piece blocks the passer's path. [NEEDS TUNING]
const PASSER_PATH_CLEAR_BONUS: (i32, i32) = (5, 20);

/// Passed pawn king proximity bonus: bonus when friendly king is near the passer.
/// Scale: per rank of proximity (closer = more bonus). Endgame only. [NEEDS TUNING]
const PASSER_KING_PROXIMITY_EG: i32 = 5;

/// Passed pawn enemy king distance bonus: bonus when enemy king is far from passer.
/// Endgame only, per rank of distance. [NEEDS TUNING]
const PASSER_ENEMY_KING_DIST_EG: i32 = 5;

// ============================================================
// Section 1: Piece-square tables
// ============================================================

type PstTable = [(i32, i32); 64];

#[rustfmt::skip]
const PST_PAWN: PstTable = [
    (0,0),(0,0),(0,0),(0,0),(0,0),(0,0),(0,0),(0,0),
    (0,0),(0,0),(0,0),(0,0),(0,0),(0,0),(0,0),(0,0),
    (5,5),(5,5),(10,10),(0,0),(0,0),(10,10),(5,5),(5,5),
    (5,5),(10,10),(15,15),(25,25),(25,25),(15,15),(10,10),(5,5),
    (10,10),(15,15),(20,20),(30,30),(30,30),(20,20),(15,15),(10,10),
    (20,20),(25,25),(30,30),(35,35),(35,35),(30,30),(25,25),(20,20),
    (40,50),(45,55),(45,55),(45,55),(45,55),(45,55),(45,55),(40,50),
    (0,0),(0,0),(0,0),(0,0),(0,0),(0,0),(0,0),(0,0),
];

#[rustfmt::skip]
const PST_KNIGHT: PstTable = [
    (-50,-30),(-40,-20),(-30,-10),(-30,-10),(-30,-10),(-30,-10),(-40,-20),(-50,-30),
    (-40,-20),(-20, -5),  (0,  0),  (0,  0),  (0,  0),  (0,  0),(-20, -5),(-40,-20),
    (-30,-10),  (0,  0),(10,  5),(15, 10),(15, 10),(10,  5),  (0,  0),(-30,-10),
    (-30,-10),  (5,  5),(15, 10),(20, 15),(20, 15),(15, 10),  (5,  5),(-30,-10),
    (-30,-10),  (0,  0),(15, 10),(20, 15),(20, 15),(15, 10),  (0,  0),(-30,-10),
    (-30,-10),  (5,  5),(10,  5),(15, 10),(15, 10),(10,  5),  (5,  5),(-30,-10),
    (-40,-20),(-20, -5),  (0,  0),  (5,  5),  (5,  5),  (0,  0),(-20, -5),(-40,-20),
    (-50,-30),(-40,-20),(-30,-10),(-30,-10),(-30,-10),(-30,-10),(-40,-20),(-50,-30),
];

#[rustfmt::skip]
const PST_BISHOP: PstTable = [
    (-20,-10),(-10,-5),(-10,-5),(-10,-5),(-10,-5),(-10,-5),(-10,-5),(-20,-10),
    (-10,-5),  (0,  0),  (0,  0),  (0,  0),  (0,  0),  (0,  0),  (0,  0),(-10,-5),
    (-10,-5),  (0,  0),  (5,  5),(10, 10),(10, 10),  (5,  5),  (0,  0),(-10,-5),
    (-10,-5),  (5,  5),  (5,  5),(10, 10),(10, 10),  (5,  5),  (5,  5),(-10,-5),
    (-10,-5),  (0,  0),(10, 10),(10, 10),(10, 10),(10, 10),  (0,  0),(-10,-5),
    (-10,-5),(10, 10),(10, 10),(10, 10),(10, 10),(10, 10),(10, 10),(-10,-5),
    (-10,-5),  (5,  0),  (0,  0),  (0,  0),  (0,  0),  (0,  0),  (5,  0),(-10,-5),
    (-20,-10),(-10,-5),(-10,-5),(-10,-5),(-10,-5),(-10,-5),(-10,-5),(-20,-10),
];

#[rustfmt::skip]
const PST_ROOK: PstTable = [
    ( 0,  0),( 0,  0),( 0,  0),( 5,  5),( 5,  5),( 0,  0),( 0,  0),( 0,  0),
    (-5,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),(-5,  0),
    (-5,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),(-5,  0),
    (-5,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),(-5,  0),
    (-5,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),(-5,  0),
    (-5,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),(-5,  0),
    ( 5, 10),( 5, 10),( 5, 10),( 5, 10),( 5, 10),( 5, 10),( 5, 10),( 5, 10),
    ( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),( 0,  0),
];

#[rustfmt::skip]
const PST_QUEEN: PstTable = [
    (-20,-10),(-10,-5),(-10,-5),( -5,-5),( -5,-5),(-10,-5),(-10,-5),(-20,-10),
    (-10,-5),  (0,  0),  (5,  0),  (0,  0),  (0,  0),  (0,  0),  (0,  0),(-10,-5),
    (-10,-5),  (5,  0),  (5,  5),  (5,  5),  (5,  5),  (5,  5),  (0,  0),(-10,-5),
    ( -5,-5),  (0,  0),  (5,  5),  (5,  5),  (5,  5),  (5,  5),  (0,  0),( -5,-5),
    ( -5,-5),  (0,  0),  (5,  5),  (5,  5),  (5,  5),  (5,  5),  (0,  0),( -5,-5),
    (-10,-5),  (5,  0),  (5,  5),  (5,  5),  (5,  5),  (5,  5),  (0,  0),(-10,-5),
    (-10,-5),  (0,  0),  (5,  0),  (0,  0),  (0,  0),  (0,  0),  (0,  0),(-10,-5),
    (-20,-10),(-10,-5),(-10,-5),( -5,-5),( -5,-5),(-10,-5),(-10,-5),(-20,-10),
];

#[rustfmt::skip]
const PST_KING: PstTable = [
    ( 20,-50),( 30,-30),( 10,-30),(  0,-30),(  0,-30),( 10,-30),( 30,-30),( 20,-50),
    ( 20,-30),(  0,-20),(  0,-20),(  0,-20),(  0,-20),(  0,-20),(  0,-20),( 20,-30),
    (-10,-10),(-20,  0),(-20,  0),(-20,  0),(-20,  0),(-20,  0),(-20,  0),(-10,-10),
    (-20,-20),(-30,-10),(-30,-10),(-40,-10),(-40,-10),(-30,-10),(-30,-10),(-20,-20),
    (-30,-20),(-40,-10),(-40,-10),(-50,-10),(-50,-10),(-40,-10),(-40,-10),(-30,-20),
    (-30,-20),(-40,-10),(-40,-10),(-50,-10),(-50,-10),(-40,-10),(-40,-10),(-30,-20),
    (-30,-20),(-40,-10),(-40,-10),(-50,-10),(-50,-10),(-40,-10),(-40,-10),(-30,-20),
    (-30,-20),(-40,-10),(-40,-10),(-50,-10),(-50,-10),(-40,-10),(-40,-10),(-30,-20),
];

fn pst_value(pt: PieceType, sq: Square, color: Color) -> (i32, i32) {
    let idx = if color == Color::White {
        sq as usize
    } else {
        let r = 7 - sq_rank(sq);
        let f = sq_file(sq);
        (r * 8 + f) as usize
    };
    match pt {
        PieceType::Pawn => PST_PAWN[idx],
        PieceType::Knight => PST_KNIGHT[idx],
        PieceType::Bishop => PST_BISHOP[idx],
        PieceType::Rook => PST_ROOK[idx],
        PieceType::Queen => PST_QUEEN[idx],
        PieceType::King => PST_KING[idx],
        PieceType::None => (0, 0),
    }
}

// ============================================================
// Section 2: Mobility tables
// ============================================================

const KNIGHT_MOBILITY: [(i32, i32); 9] = [
    (-20, -10),
    (-10, -5),
    (0, 0),
    (4, 4),
    (8, 8),
    (12, 10),
    (16, 12),
    (20, 14),
    (24, 16),
];
const BISHOP_MOBILITY: [(i32, i32); 14] = [
    (-20, -10),
    (-10, -5),
    (0, 0),
    (3, 3),
    (6, 5),
    (9, 7),
    (12, 9),
    (15, 11),
    (18, 13),
    (20, 15),
    (22, 17),
    (24, 19),
    (26, 21),
    (28, 23),
];
const ROOK_MOBILITY: [(i32, i32); 15] = [
    (-15, -10),
    (-8, -5),
    (0, 0),
    (2, 2),
    (4, 4),
    (6, 6),
    (8, 8),
    (10, 10),
    (12, 12),
    (14, 14),
    (16, 16),
    (18, 18),
    (20, 20),
    (22, 22),
    (24, 24),
];
const QUEEN_MOBILITY: [(i32, i32); 28] = [
    (-10, -5),
    (-5, -2),
    (0, 0),
    (1, 1),
    (2, 2),
    (3, 3),
    (4, 4),
    (5, 5),
    (6, 6),
    (7, 7),
    (8, 8),
    (9, 9),
    (10, 10),
    (11, 11),
    (12, 12),
    (13, 13),
    (14, 14),
    (15, 15),
    (16, 16),
    (17, 17),
    (18, 18),
    (19, 19),
    (20, 20),
    (21, 21),
    (22, 22),
    (23, 23),
    (24, 24),
    (25, 25),
];

// ============================================================
// Section 3: Main evaluation
// ============================================================

pub struct EvalContext<'a> {
    pub atk: &'a AttackTables,
    pub options: EngineOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvalBreakdown {
    pub phase: i32,
    pub material_mg: i32,
    pub material_eg: i32,
    pub material_cp: i32,
    pub pst_mg: i32,
    pub pst_eg: i32,
    pub pst_cp: i32,
    pub mobility_mg: i32,
    pub mobility_eg: i32,
    pub mobility_cp: i32,
    pub mobility_white: u32,
    pub mobility_black: u32,
    pub pawn_structure_mg: i32,
    pub pawn_structure_eg: i32,
    pub pawn_structure_cp: i32,
    pub king_safety_mg: i32,
    pub king_safety_eg: i32,
    pub king_safety_cp: i32,
    pub freedom: i32,
    pub trade_down_mg: i32,
    pub trade_down_eg: i32,
    pub trade_down_cp: i32,
    pub weak_squares_mg: i32,
    pub weak_squares_eg: i32,
    pub weak_squares_cp: i32,
    pub coordination_mg: i32,
    pub coordination_eg: i32,
    pub coordination_cp: i32,
    pub advanced_pawns_mg: i32,
    pub advanced_pawns_eg: i32,
    pub advanced_pawns_cp: i32,
    pub white_score: i32,
    pub side_to_move_score: i32,
}

/// Evaluate the position. Returns score in centipawns from the perspective of the side to move.
pub fn evaluate(board: &Board, ctx: &EvalContext) -> Score {
    evaluate_breakdown(board, ctx).side_to_move_score
}

pub fn evaluate_breakdown(board: &Board, ctx: &EvalContext) -> EvalBreakdown {
    let phase = compute_phase(board);

    let mut mg_score = 0i32;
    let mut eg_score = 0i32;

    let ((mat_mg, mat_eg), (pst_mg, pst_eg)) = material_and_pst(board);
    let (mat_mg, mat_eg) = scale_score_pair((mat_mg, mat_eg), ctx.options.eval.material_scale);
    mg_score += mat_mg;
    eg_score += mat_eg;
    let (pst_mg, pst_eg) = scale_score_pair((pst_mg, pst_eg), ctx.options.eval.pst_scale);
    mg_score += pst_mg;
    eg_score += pst_eg;

    let (mob_mg, mob_eg) = mobility_and_activity(board, ctx);
    let (mob_mg, mob_eg) = scale_score_pair((mob_mg, mob_eg), ctx.options.eval.mobility_scale);
    let mobility_white = side_mobility(board, ctx, Color::White);
    let mobility_black = side_mobility(board, ctx, Color::Black);
    mg_score += mob_mg;
    eg_score += mob_eg;

    let (pawn_mg, pawn_eg) = pawn_structure(board);
    let (pawn_mg, pawn_eg) =
        scale_score_pair((pawn_mg, pawn_eg), ctx.options.eval.pawn_structure_scale);
    mg_score += pawn_mg;
    eg_score += pawn_eg;

    let (ks_mg, ks_eg) = king_safety(board, ctx);
    let (ks_mg, ks_eg) = scale_score_pair((ks_mg, ks_eg), ctx.options.eval.king_safety_scale);
    mg_score += ks_mg;
    eg_score += ks_eg;

    let freedom = 0;
    let (trade_mg, trade_eg) = (0, 0);
    let (ws_mg, ws_eg) = (0, 0);
    let (pc_mg, pc_eg) = (0, 0);
    let (ap_mg, ap_eg) = (0, 0);

    let score = blend_phase(mg_score, eg_score, phase);

    let tempo = TEMPO_BONUS;
    let side_sign = if board.side == Color::White { 1 } else { -1 };
    EvalBreakdown {
        phase,
        material_mg: mat_mg,
        material_eg: mat_eg,
        material_cp: blend_phase(mat_mg, mat_eg, phase),
        pst_mg,
        pst_eg,
        pst_cp: blend_phase(pst_mg, pst_eg, phase),
        mobility_mg: mob_mg,
        mobility_eg: mob_eg,
        mobility_cp: blend_phase(mob_mg, mob_eg, phase),
        mobility_white,
        mobility_black,
        pawn_structure_mg: pawn_mg,
        pawn_structure_eg: pawn_eg,
        pawn_structure_cp: blend_phase(pawn_mg, pawn_eg, phase),
        king_safety_mg: ks_mg,
        king_safety_eg: ks_eg,
        king_safety_cp: blend_phase(ks_mg, ks_eg, phase),
        freedom,
        trade_down_mg: trade_mg,
        trade_down_eg: trade_eg,
        trade_down_cp: blend_phase(trade_mg, trade_eg, phase),
        weak_squares_mg: ws_mg,
        weak_squares_eg: ws_eg,
        weak_squares_cp: blend_phase(ws_mg, ws_eg, phase),
        coordination_mg: pc_mg,
        coordination_eg: pc_eg,
        coordination_cp: blend_phase(pc_mg, pc_eg, phase),
        advanced_pawns_mg: ap_mg,
        advanced_pawns_eg: ap_eg,
        advanced_pawns_cp: blend_phase(ap_mg, ap_eg, phase),
        white_score: score,
        side_to_move_score: score * side_sign + tempo,
    }
}

fn blend_phase(mg: i32, eg: i32, phase: i32) -> i32 {
    (mg * phase + eg * (256 - phase)) / 256
}

fn compute_phase(board: &Board) -> i32 {
    let w = board.non_pawn_material(Color::White);
    let b = board.non_pawn_material(Color::Black);
    game_phase(w + b)
}

fn material_and_pst(board: &Board) -> ((i32, i32), (i32, i32)) {
    let mut mat_mg = 0i32;
    let mut mat_eg = 0i32;
    let mut pst_mg_total = 0i32;
    let mut pst_eg_total = 0i32;
    for c in [Color::White, Color::Black] {
        let sign = if c == Color::White { 1 } else { -1 };
        let ci = c as usize;
        for pt_u8 in 0..6u8 {
            let pt = PieceType::from_u8(pt_u8);
            let mut pieces = board.pieces[ci][pt_u8 as usize];
            while pieces != 0 {
                let sq = bb_pop_lsb(&mut pieces);
                let mat = pt.material_value();
                let (pst_mg, pst_eg) = pst_value(pt, sq, c);
                mat_mg += sign * mat;
                mat_eg += sign * mat;
                pst_mg_total += sign * pst_mg;
                pst_eg_total += sign * pst_eg;
            }
        }
    }
    ((mat_mg, mat_eg), (pst_mg_total, pst_eg_total))
}

// ============================================================
// Section 4: Mobility and activity
// ============================================================
/// Rook file bonus: open file, semi-open file, or nothing.
fn rook_file_bonus(file_bb: Bb, our_pawns: Bb, their_pawns: Bb) -> (i32, i32) {
    if our_pawns & file_bb != 0 {
        return (0, 0);
    }
    if their_pawns & file_bb == 0 {
        (ROOK_OPEN_FILE_BONUS.0, ROOK_OPEN_FILE_BONUS.1)
    } else {
        (ROOK_SEMI_OPEN_FILE_BONUS.0, ROOK_SEMI_OPEN_FILE_BONUS.1)
    }
}

fn mobility_and_activity(board: &Board, ctx: &EvalContext) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;

    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let ci = color as usize;
        let occ = board.occ_all;
        let our_occ = board.occ[ci];

        let their_pawn_attacks = if color == Color::White {
            pawn_attacks_black(board.pieces[1][PieceType::Pawn as usize])
        } else {
            pawn_attacks_white(board.pieces[0][PieceType::Pawn as usize])
        };

        // Knights
        let mut knights = board.pieces[ci][PieceType::Knight as usize];
        while knights != 0 {
            let sq = bb_pop_lsb(&mut knights);
            let atk = ctx.atk.knight[sq as usize];
            let mob = (atk & !our_occ & !their_pawn_attacks).count_ones() as usize;
            let mob = mob.min(8);
            mg += sign * KNIGHT_MOBILITY[mob].0;
            eg += sign * KNIGHT_MOBILITY[mob].1;
            mg += sign * outpost_bonus(sq, color, their_pawn_attacks, board);
        }

        // Bishops
        let mut bishops = board.pieces[ci][PieceType::Bishop as usize];
        while bishops != 0 {
            let sq = bb_pop_lsb(&mut bishops);
            let atk = ctx.atk.bishop_attacks(sq, occ);
            let mob = (atk & !our_occ & !their_pawn_attacks).count_ones() as usize;
            let mob = mob.min(13);
            mg += sign * BISHOP_MOBILITY[mob].0;
            eg += sign * BISHOP_MOBILITY[mob].1;
        }

        // Bishop pair
        if board.pieces[ci][PieceType::Bishop as usize].count_ones() >= 2 {
            mg += sign * BISHOP_PAIR_BONUS.0;
            eg += sign * BISHOP_PAIR_BONUS.1;
        }

        // Rooks
        let mut rooks = board.pieces[ci][PieceType::Rook as usize];
        while rooks != 0 {
            let sq = bb_pop_lsb(&mut rooks);
            let atk = ctx.atk.rook_attacks(sq, occ);
            let mob = (atk & !our_occ).count_ones() as usize;
            let mob = mob.min(14);
            mg += sign * ROOK_MOBILITY[mob].0;
            eg += sign * ROOK_MOBILITY[mob].1;

            let file_bb = BB_FILES[sq_file(sq) as usize];
            let our_pawns = board.pieces[ci][PieceType::Pawn as usize];
            let their_pawns = board.pieces[color.flip() as usize][PieceType::Pawn as usize];
            let (rk_mg, rk_eg) = rook_file_bonus(file_bb, our_pawns, their_pawns);
            mg += sign * rk_mg;
            eg += sign * rk_eg;

            let seventh_rank = if color == Color::White {
                BB_RANK_7
            } else {
                BB_RANK_2
            };
            if bb(sq) & seventh_rank != 0 {
                mg += sign * ROOK_ON_SEVENTH_BONUS.0;
                eg += sign * ROOK_ON_SEVENTH_BONUS.1;
            }
        }

        // Queens
        let mut queens = board.pieces[ci][PieceType::Queen as usize];
        while queens != 0 {
            let sq = bb_pop_lsb(&mut queens);
            let atk = ctx.atk.queen_attacks(sq, occ);
            let mob = (atk & !our_occ & !their_pawn_attacks).count_ones() as usize;
            let mob = mob.min(27);
            mg += sign * QUEEN_MOBILITY[mob].0;
            eg += sign * QUEEN_MOBILITY[mob].1;
        }
    }

    (mg, eg)
}

fn outpost_bonus(sq: Square, color: Color, their_pawn_attacks: Bb, board: &Board) -> i32 {
    if bb(sq) & their_pawn_attacks != 0 {
        return 0;
    }
    let r = sq_rank(sq);
    let in_outpost_zone = if color == Color::White {
        (3..=5).contains(&r)
    } else {
        (2..=4).contains(&r)
    };
    if !in_outpost_zone {
        return 0;
    }
    let our_pawn_attacks = if color == Color::White {
        pawn_attacks_white(board.pieces[color as usize][PieceType::Pawn as usize])
    } else {
        pawn_attacks_black(board.pieces[color as usize][PieceType::Pawn as usize])
    };
    if our_pawn_attacks & bb(sq) != 0 {
        OUTPOST_SUPPORTED
    } else {
        OUTPOST_UNSUPPORTED
    }
}

// ============================================================
// Section 5: Mobility diagnostics
// ============================================================

/// Total pseudo-legal mobility for one side (pawns incl. pushes/captures, pieces, king).
pub(crate) fn side_mobility(board: &Board, ctx: &EvalContext, color: Color) -> u32 {
    let ci = color as usize;
    let oi = color.flip() as usize;
    let occ = board.occ_all;
    let our_occ = board.occ[ci];

    let mut mobility = 0u32;

    // Pawns: pushes + captures of opponent pieces
    let pawns = board.pieces[ci][PieceType::Pawn as usize];
    if color == Color::White {
        mobility += ((pawns << 8) & !occ).count_ones();
        mobility += (((pawns << 8) & !occ & BB_RANK_3) << 8 & !occ).count_ones();
        mobility += ((pawns << 9) & !BB_FILE_A & board.occ[oi]).count_ones();
        mobility += ((pawns << 7) & !BB_FILE_H & board.occ[oi]).count_ones();
    } else {
        mobility += ((pawns >> 8) & !occ).count_ones();
        mobility += (((pawns >> 8) & !occ & BB_RANK_6) >> 8 & !occ).count_ones();
        mobility += ((pawns >> 7) & !BB_FILE_A & board.occ[oi]).count_ones();
        mobility += ((pawns >> 9) & !BB_FILE_H & board.occ[oi]).count_ones();
    }

    // Knights
    let mut knights = board.pieces[ci][PieceType::Knight as usize];
    while knights != 0 {
        let sq = bb_pop_lsb(&mut knights);
        mobility += (ctx.atk.knight[sq as usize] & !our_occ).count_ones();
    }

    // Bishops
    let mut bishops = board.pieces[ci][PieceType::Bishop as usize];
    while bishops != 0 {
        let sq = bb_pop_lsb(&mut bishops);
        mobility += (ctx.atk.bishop_attacks(sq, occ) & !our_occ).count_ones();
    }

    // Rooks
    let mut rooks = board.pieces[ci][PieceType::Rook as usize];
    while rooks != 0 {
        let sq = bb_pop_lsb(&mut rooks);
        mobility += (ctx.atk.rook_attacks(sq, occ) & !our_occ).count_ones();
    }

    // Queens
    let mut queens = board.pieces[ci][PieceType::Queen as usize];
    while queens != 0 {
        let sq = bb_pop_lsb(&mut queens);
        mobility += (ctx.atk.queen_attacks(sq, occ) & !our_occ).count_ones();
    }

    // King
    let king_sq = board.king_sq[ci];
    if king_sq != NO_SQUARE {
        mobility += (ctx.atk.king[king_sq as usize] & !our_occ).count_ones();
    }

    mobility
}

/// Build a bitboard mask of ranks ahead of `rank` for the given color, intersected with `file_mask`.
fn ranks_ahead(color: Color, rank: u8, file_mask: Bb) -> Bb {
    let mut mask = 0u64;
    if color == Color::White {
        for r in (rank + 1)..8 {
            mask |= BB_RANKS[r as usize];
        }
    } else {
        for r in 0..rank {
            mask |= BB_RANKS[r as usize];
        }
    }
    mask & file_mask
}

/// Build a bitboard mask of ranks behind (or equal to) `rank` for the given color, intersected with `file_mask`.
fn ranks_behind_inclusive(color: Color, rank: u8, file_mask: Bb) -> Bb {
    let mut mask = 0u64;
    if color == Color::White {
        for r in 0..=rank {
            mask |= BB_RANKS[r as usize];
        }
    } else {
        for r in rank..8 {
            mask |= BB_RANKS[r as usize];
        }
    }
    mask & file_mask
}

struct PassedPawnContext {
    sq: Square,
    rank: u8,
    file: u8,
    file_bb: Bb,
    adj_files: Bb,
    our_pawns: Bb,
    their_pawns: Bb,
    promo_dist: u8,
}

/// Evaluate a single passed pawn's bonuses (path clear, king proximity, connected, rook behind).
fn passed_pawn_bonuses(board: &Board, color: Color, passed: PassedPawnContext) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;
    let ci = color as usize;
    let ti = color.flip() as usize;
    let sign = if color == Color::White { 1 } else { -1 };
    let adv = (7 - passed.promo_dist) as usize;

    mg += sign * PASSED_PAWN_BONUS_MG[adv.min(7)];
    eg += sign * PASSED_PAWN_BONUS_EG[adv.min(7)];

    // Path clear bonus
    let path_mask = ranks_ahead(color, passed.rank, passed.file_bb);
    if board.occ_all & path_mask == 0 {
        mg += sign * PASSER_PATH_CLEAR_BONUS.0;
        eg += sign * PASSER_PATH_CLEAR_BONUS.1;
    }

    // King proximity
    let our_king = board.king_sq[ci];
    let their_king = board.king_sq[ti];
    if our_king != NO_SQUARE && their_king != NO_SQUARE {
        let our_dist = chebyshev_distance(our_king, passed.sq);
        let their_dist = chebyshev_distance(their_king, passed.sq);
        eg += sign * (4i32 - our_dist as i32).max(0) * PASSER_KING_PROXIMITY_EG;
        eg += sign * (their_dist as i32 - 3).max(0) * PASSER_ENEMY_KING_DIST_EG;
    }

    // Connected passed pawns
    let mut adj_pawns = passed.adj_files & passed.our_pawns;
    while adj_pawns != 0 {
        let adj_sq = bb_pop_lsb(&mut adj_pawns);
        let adj_file_bb = BB_FILES[sq_file(adj_sq) as usize];
        let adj_rank = sq_rank(adj_sq);
        let adj_ahead = ranks_ahead(
            color,
            adj_rank,
            adj_file_bb | BB_FILES[passed.file as usize],
        );
        if passed.their_pawns & adj_ahead == 0 {
            mg += sign * CONNECTED_PASSER_BONUS.0;
            eg += sign * CONNECTED_PASSER_BONUS.1;
            break;
        }
    }

    // Rook behind passed pawn
    let rooks = board.pieces[ci][PieceType::Rook as usize];
    let behind_mask = ranks_behind_inclusive(color, passed.rank, passed.file_bb);
    if rooks & behind_mask != 0 {
        mg += sign * ROOK_BEHIND_PASSER_BONUS.0;
        eg += sign * ROOK_BEHIND_PASSER_BONUS.1;
    }

    (mg, eg)
}

/// Check if a pawn is backward: advance square attacked by enemy, no friendly support behind.
fn is_backward_pawn(
    color: Color,
    sq: Square,
    rank: u8,
    adj_files: Bb,
    our_pawns: Bb,
    their_pawn_attacks_bb: Bb,
) -> bool {
    // Must not be isolated (handled separately)
    if our_pawns & adj_files == 0 {
        return false;
    }

    let advance_sq = if color == Color::White {
        if rank >= 7 {
            return false;
        }
        sq + 8
    } else {
        if rank == 0 {
            return false;
        }
        sq - 8
    };

    // Advance square must be attacked by enemy pawn
    if bb(advance_sq) & their_pawn_attacks_bb == 0 {
        return false;
    }

    // No friendly pawn on adjacent files behind or equal rank can support
    let support_mask = ranks_behind_inclusive(color, rank, adj_files);
    our_pawns & support_mask == 0
}

// ============================================================
// Section 6: Pawn structure
// ============================================================

fn pawn_structure(board: &Board) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;

    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let ci = color as usize;
        let ti = color.flip() as usize;
        let our_pawns = board.pieces[ci][PieceType::Pawn as usize];
        let their_pawns = board.pieces[ti][PieceType::Pawn as usize];

        let their_pawn_attacks_bb = if color == Color::White {
            pawn_attacks_black(their_pawns)
        } else {
            pawn_attacks_white(their_pawns)
        };

        let mut pawns = our_pawns;
        while pawns != 0 {
            let sq = bb_pop_lsb(&mut pawns);
            let file = sq_file(sq);
            let rank = sq_rank(sq);
            let file_bb = BB_FILES[file as usize];
            let left_file = if file > 0 {
                BB_FILES[(file - 1) as usize]
            } else {
                0
            };
            let right_file = if file < 7 {
                BB_FILES[(file + 1) as usize]
            } else {
                0
            };
            let adj_files = left_file | right_file;

            // Doubled pawns
            if (our_pawns & file_bb).count_ones() > 1 {
                mg += sign * DOUBLED_PAWN_PENALTY.0;
                eg += sign * DOUBLED_PAWN_PENALTY.1;
            }

            // Isolated pawn
            if our_pawns & adj_files == 0 {
                mg += sign * ISOLATED_PAWN_PENALTY.0;
                eg += sign * ISOLATED_PAWN_PENALTY.1;
            }

            // Passed pawn
            let ahead_mask = ranks_ahead(color, rank, file_bb | adj_files);
            let promo_dist = if color == Color::White {
                7 - rank
            } else {
                rank
            };
            if their_pawns & ahead_mask == 0 && our_pawns & ranks_ahead(color, rank, file_bb) == 0 {
                let passed = PassedPawnContext {
                    sq,
                    rank,
                    file,
                    file_bb,
                    adj_files,
                    our_pawns,
                    their_pawns,
                    promo_dist,
                };
                let (pmg, peg) = passed_pawn_bonuses(board, color, passed);
                mg += pmg;
                eg += peg;
            }

            // Backward pawn
            if is_backward_pawn(color, sq, rank, adj_files, our_pawns, their_pawn_attacks_bb) {
                mg += sign * BACKWARD_PAWN_PENALTY.0;
                eg += sign * BACKWARD_PAWN_PENALTY.1;
            }
        }

        // Pawn chain
        let protected = if color == Color::White {
            pawn_attacks_white(our_pawns) & our_pawns
        } else {
            pawn_attacks_black(our_pawns) & our_pawns
        };
        let chain_count = protected.count_ones() as i32;
        mg += sign * chain_count * PAWN_CHAIN_BONUS.0;
        eg += sign * chain_count * PAWN_CHAIN_BONUS.1;
    }

    (mg, eg)
}

// ============================================================
// Section 7: King safety
// ============================================================

fn king_safety(board: &Board, ctx: &EvalContext) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;

    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let ci = color as usize;
        let ti = color.flip() as usize;
        let king_sq = board.king_sq[ci];
        if king_sq == NO_SQUARE {
            continue;
        }

        // Pawn shield
        let king_file = sq_file(king_sq);
        let shield_rank = if color == Color::White {
            BB_RANK_2 | BB_RANK_3
        } else {
            BB_RANK_6 | BB_RANK_7
        };
        let shield_files = BB_FILES[king_file as usize]
            | (if king_file > 0 {
                BB_FILES[(king_file - 1) as usize]
            } else {
                0
            })
            | (if king_file < 7 {
                BB_FILES[(king_file + 1) as usize]
            } else {
                0
            });
        let shield = board.pieces[ci][PieceType::Pawn as usize] & shield_rank & shield_files;
        let shield_count = shield.count_ones() as i32;
        mg += sign * (shield_count * PAWN_SHIELD_PER_PAWN - PAWN_SHIELD_BASE_PENALTY);

        // King zone attacks
        let king_zone = ctx.atk.king[king_sq as usize] | bb(king_sq);
        let mut attack_units = 0i32;
        let occ = board.occ_all;

        let mut their_knights = board.pieces[ti][PieceType::Knight as usize];
        while their_knights != 0 {
            let sq = bb_pop_lsb(&mut their_knights);
            if ctx.atk.knight[sq as usize] & king_zone != 0 {
                attack_units += KING_ATTACK_WEIGHT_KNIGHT;
            }
        }
        let mut their_bishops = board.pieces[ti][PieceType::Bishop as usize];
        while their_bishops != 0 {
            let sq = bb_pop_lsb(&mut their_bishops);
            if ctx.atk.bishop_attacks(sq, occ) & king_zone != 0 {
                attack_units += KING_ATTACK_WEIGHT_BISHOP;
            }
        }
        let mut their_rooks = board.pieces[ti][PieceType::Rook as usize];
        while their_rooks != 0 {
            let sq = bb_pop_lsb(&mut their_rooks);
            if ctx.atk.rook_attacks(sq, occ) & king_zone != 0 {
                attack_units += KING_ATTACK_WEIGHT_ROOK;
            }
        }
        let mut their_queens = board.pieces[ti][PieceType::Queen as usize];
        while their_queens != 0 {
            let sq = bb_pop_lsb(&mut their_queens);
            if ctx.atk.queen_attacks(sq, occ) & king_zone != 0 {
                attack_units += KING_ATTACK_WEIGHT_QUEEN;
            }
        }

        let penalty = KING_SAFETY_TABLE
            .iter()
            .find(|&&(max_units, _)| attack_units <= max_units)
            .map(|&(_, p)| p)
            .unwrap_or(230);
        mg += sign * (-penalty);

        // King centralization bonus (endgame only)
        let king_rank = sq_rank(king_sq) as i32;
        let center_dist = (3 - king_file as i32)
            .abs()
            .min((4 - king_file as i32).abs())
            + (3 - king_rank).abs().min((4 - king_rank).abs());
        eg += sign * (3 - center_dist).max(0) * KING_CENTRALIZATION_EG;
    }

    (mg, eg)
}

/// Chebyshev (king) distance between two squares
fn chebyshev_distance(a: Square, b: Square) -> u8 {
    let df = (sq_file(a) as i8 - sq_file(b) as i8).unsigned_abs();
    let dr = (sq_rank(a) as i8 - sq_rank(b) as i8).unsigned_abs();
    df.max(dr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passer_bonus_for(fen: &str, color: Color, sq: Square) -> (i32, i32) {
        let board = Board::from_fen(fen).unwrap();
        let ci = color as usize;
        let rank = sq_rank(sq);
        let file = sq_file(sq);
        let file_bb = BB_FILES[file as usize];
        let adj_files = (if file > 0 {
            BB_FILES[(file - 1) as usize]
        } else {
            0
        }) | (if file < 7 {
            BB_FILES[(file + 1) as usize]
        } else {
            0
        });
        let promo_dist = if color == Color::White {
            7 - rank
        } else {
            rank
        };

        let passed = PassedPawnContext {
            sq,
            rank,
            file,
            file_bb,
            adj_files,
            our_pawns: board.pieces[ci][PieceType::Pawn as usize],
            their_pawns: board.pieces[color.flip() as usize][PieceType::Pawn as usize],
            promo_dist,
        };

        passed_pawn_bonuses(&board, color, passed)
    }

    #[test]
    fn rook_behind_white_passer_is_rewarded() {
        let without_rook = passer_bonus_for("8/8/8/4P3/8/8/8/8 w - - 0 1", Color::White, E5);
        let with_rook = passer_bonus_for("8/8/8/4P3/8/8/8/4R3 w - - 0 1", Color::White, E5);

        assert_eq!(
            (with_rook.0 - without_rook.0, with_rook.1 - without_rook.1),
            ROOK_BEHIND_PASSER_BONUS,
        );
    }

    #[test]
    fn rook_in_front_of_white_passer_is_not_rewarded_as_behind() {
        let bishop_in_front = passer_bonus_for("8/8/4B3/4P3/8/8/8/8 w - - 0 1", Color::White, E5);
        let rook_in_front = passer_bonus_for("8/8/4R3/4P3/8/8/8/8 w - - 0 1", Color::White, E5);

        assert_eq!(rook_in_front, bishop_in_front);
    }

    #[test]
    fn rook_behind_black_passer_is_rewarded() {
        let without_rook = passer_bonus_for("8/8/8/4p3/8/8/8/8 b - - 0 1", Color::Black, E5);
        let with_rook = passer_bonus_for("4r3/8/8/4p3/8/8/8/8 b - - 0 1", Color::Black, E5);

        assert_eq!(
            (with_rook.0 - without_rook.0, with_rook.1 - without_rook.1),
            (-ROOK_BEHIND_PASSER_BONUS.0, -ROOK_BEHIND_PASSER_BONUS.1),
        );
    }

    #[test]
    fn rook_in_front_of_black_passer_is_not_rewarded_as_behind() {
        let bishop_in_front = passer_bonus_for("8/8/8/4p3/4b3/8/8/8 b - - 0 1", Color::Black, E5);
        let rook_in_front = passer_bonus_for("8/8/8/4p3/4r3/8/8/8 b - - 0 1", Color::Black, E5);

        assert_eq!(rook_in_front, bishop_in_front);
    }
}
