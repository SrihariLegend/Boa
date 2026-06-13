// ============================================================
// eval.rs — Karpov-style evaluation
//
// Philosophy: measure how much freedom the opponent has.
// When opponent freedom approaches zero, mistakes become inevitable.
//
// Score is always from the perspective of the side to move (negamax).
//
// Structure:
//   1. Piece-square tables (midgame + endgame, tapered)
//   2. Pawn structure (passed, isolated, doubled, chains)
//   3. Mobility — our mobility bonus + opponent mobility penalty (the Karpov squeeze)
//   4. Piece activity (outposts, rooks on open files, bishop pair)
//   5. King safety
//   6. Freedom metric (signature Karpov term)
//   7. Space evaluation (Karpov space advantage)
//   8. Trade-down bonus (simplify when ahead — Karpov technique)
//   9. Bad bishop detection (bishop blocked by own pawns)
//  10. Weak square complex (holes in pawn structure — Karpov exploitation)
//  11. Good knight vs bad bishop (minor piece imbalance)
//  12. Prophylaxis (opponent pawn break prevention)
//  13. Piece coordination (harmony between pieces)
//  14. Passed pawn advancement safety (clear path + king support)
// ============================================================

#![allow(dead_code)]

use crate::types::*;
use crate::board::Board;
use crate::movegen::{AttackTables, pawn_attacks_white, pawn_attacks_black};

// ---- Evaluation tuning constants ----
// Sources: Stockfish (SF), Chess Programming Wiki (CPW), or marked [NEEDS TUNING].
//
// Piece values are defined in types.rs (Kaufman values: P=100, N=320, B=330, R=500, Q=900).

/// Bishop pair bonus (mg, eg). Having two bishops is worth extra material.
/// SF uses ~30/50 (tuned). CPW recommends 25-50. [NEEDS TUNING]
const BISHOP_PAIR_BONUS: (i32, i32) = (25, 80);

/// Rook on fully open file (no pawns of either color). (mg, eg)
/// SF: ~20/7. CPW: 15-25. [NEEDS TUNING]
const ROOK_OPEN_FILE_BONUS: (i32, i32) = (25, 5);

/// Rook on semi-open file (no friendly pawns). (mg, eg)
/// SF: ~7/6. CPW: 8-15. [NEEDS TUNING]
const ROOK_SEMI_OPEN_FILE_BONUS: (i32, i32) = (19, 8);

/// Rook on 7th rank bonus. Strong in both phases.
/// SF: ~15-30 depending on context. CPW: 20-30. [NEEDS TUNING]
const ROOK_ON_SEVENTH_BONUS: (i32, i32) = (50, 35);

/// Knight outpost bonus when supported/unsupported by own pawn.
/// An outpost is a square on ranks 4-6 not attackable by enemy pawns.
/// SF: ~30-50 for supported outposts. These are conservative. [NEEDS TUNING]
/// (Values were swapped — a supported outpost must outscore an unsupported one.)
const OUTPOST_SUPPORTED: i32 = 10;
const OUTPOST_UNSUPPORTED: i32 = 5;

/// Tempo bonus: side-to-move advantage in centipawns.
/// SF uses ~28 (tuned). 15 is conservative. [NEEDS TUNING]
const TEMPO_BONUS: i32 = 24;

/// Doubled pawn penalty (mg, eg). Two pawns on same file.
/// SF: ~-5/-20 (file-dependent). CPW: -10 to -20. [NEEDS TUNING]
const DOUBLED_PAWN_PENALTY: (i32, i32) = (-13, 0);

/// Isolated pawn penalty (mg, eg). No friendly pawns on adjacent files.
/// SF: ~-10/-20. CPW: -15 to -25. [NEEDS TUNING]
const ISOLATED_PAWN_PENALTY: (i32, i32) = (-15, -40);

/// Backward pawn penalty (mg, eg). Pawn on starting rank with no adjacent support.
/// Less studied than isolated. SF has complex backward pawn logic. [NEEDS TUNING]
const BACKWARD_PAWN_PENALTY: (i32, i32) = (-23, -30);

/// Pawn chain bonus per protected pawn (mg, eg).
/// Pawns defending each other. SF: ~3-5. [NEEDS TUNING]
const PAWN_CHAIN_BONUS: (i32, i32) = (6, 15);

/// Passed pawn bonus tables indexed by advancement (distance from promotion).
/// Values increase exponentially as pawn advances. Shape follows SF/CPW convention. [NEEDS TUNING]
const PASSED_PAWN_BONUS_MG: [i32; 8] = [0, 2, 0, 5, 15, 75, 70, 0];
const PASSED_PAWN_BONUS_EG: [i32; 8] = [0, 10, 5, 20, 65, 95, 115, 0];

/// Pawn shield: bonus per shielding pawn, with a base penalty for exposed king.
/// shield_score = count * PER_PAWN - BASE_PENALTY
/// With 3 shield pawns: 3*10 - 30 = 0 (neutral). 0 pawns: -30. [NEEDS TUNING]
const PAWN_SHIELD_PER_PAWN: i32 = 22;
const PAWN_SHIELD_BASE_PENALTY: i32 = 30;

/// King zone attack unit weights by piece type.
/// Each piece attacking the king zone contributes this many "attack units".
/// Inspired by CPW safety tables. Queens count most, pawns/kings not counted. [NEEDS TUNING]
const KING_ATTACK_WEIGHT_KNIGHT: i32 = 2;
const KING_ATTACK_WEIGHT_BISHOP: i32 = 3;
const KING_ATTACK_WEIGHT_ROOK: i32 = 1;
const KING_ATTACK_WEIGHT_QUEEN: i32 = 5;

/// King safety penalty table: maps attack_units to penalty.
/// Loosely follows the CPW safety table shape (quadratic-ish growth). [NEEDS TUNING]
const KING_SAFETY_TABLE: [(i32, i32); 7] = [
    // (max_attack_units, penalty)
    (2, 0), (5, 20), (8, 50), (11, 80), (15, 120), (20, 170), (i32::MAX, 230),
];

/// Freedom metric squeeze bonuses.
/// These reward positions where opponent mobility is severely restricted.
/// The tiered structure reflects that restriction becomes exponentially
/// more valuable as mobility approaches zero. [NEEDS TUNING]
const SQUEEZE_TOTAL_LOCKDOWN: i32 = 80;  // 0 moves
const SQUEEZE_SEVERE_BASE: i32 = 20;     // 1-5 moves: 60 + (5-mob)*4
const SQUEEZE_SEVERE_PER_MOVE: i32 = 4;
const SQUEEZE_MODERATE_BASE: i32 = 10;   // 6-15 moves: 30 + (15-mob)*3
const SQUEEZE_MODERATE_PER_MOVE: i32 = 1;

/// Space evaluation: count squares controlled behind our pawn chain.
/// Karpov was famous for gradually gaining space advantage.
/// Space is more valuable in the middlegame. [NEEDS TUNING]
const SPACE_WEIGHT_MG: i32 = 2;
const SPACE_WEIGHT_EG: i32 = 0;
/// Only count space in the center files (C-F) where it matters most.
const SPACE_CENTER_FILES: Bb = 0x3C3C3C3C3C3C3C3C; // files C through F

/// Trade-down bonus: when ahead in material, reward exchanging pieces.
/// Karpov would grind down into won endgames by simplifying.
/// Bonus per centipawn of material advantage, scaled by pieces traded. [NEEDS TUNING]
const TRADE_DOWN_BONUS_PER_100CP: i32 = 15;

/// Bad bishop: penalty per own pawn on same color squares as bishop.
/// A bishop blocked by its own pawns is a Karpov-style weakness to exploit. [NEEDS TUNING]
const BAD_BISHOP_PENALTY_PER_PAWN: (i32, i32) = (-3, -5);

/// Rook behind passed pawn bonus. Rooks belong behind passers (Tarrasch rule).
/// Applies to both own and enemy passed pawns. [NEEDS TUNING]
const ROOK_BEHIND_PASSER_BONUS: (i32, i32) = (5, 10);

/// King centralization in endgame: bonus per rank/file closer to center.
/// Karpov's endgame technique relied on active king. [NEEDS TUNING]
const KING_CENTRALIZATION_EG: i32 = 15;

/// Connected passed pawn bonus multiplier.
/// Two passed pawns on adjacent files supporting each other. [NEEDS TUNING]
const CONNECTED_PASSER_BONUS: (i32, i32) = (0, 5);

/// Passed pawn path clear bonus: extra bonus when no piece blocks the passer's path.
/// Karpov converted passers by ensuring the path was clear. [NEEDS TUNING]
const PASSER_PATH_CLEAR_BONUS: (i32, i32) = (2, 15);

/// Passed pawn king proximity bonus: bonus when friendly king is near the passer.
/// Scale: per rank of proximity (closer = more bonus). Endgame only. [NEEDS TUNING]
const PASSER_KING_PROXIMITY_EG: i32 = 15;

/// Passed pawn enemy king distance bonus: bonus when enemy king is far from passer.
/// Endgame only, per rank of distance. [NEEDS TUNING]
const PASSER_ENEMY_KING_DIST_EG: i32 = 10;

/// Weak square bonus: reward for controlling holes in opponent's pawn structure.
/// A "hole" is a square that can never be defended by enemy pawns.
/// Karpov was the supreme exploiter of weak-square complexes. [NEEDS TUNING]
const WEAK_SQUARE_CONTROL_BONUS: (i32, i32) = (1, 1);

/// Weak square occupation bonus: knight on a hole is especially strong.
/// SF: outpost bonuses are 30-50. This is specifically for holes. [NEEDS TUNING]
const WEAK_SQUARE_KNIGHT_BONUS: (i32, i32) = (20, 9);

/// Good knight vs bad bishop: bonus when we have a knight and they have a
/// bishop in a closed/semi-closed position. Karpov engineered these imbalances.
/// Triggered when center has 4+ pawns (closed). [NEEDS TUNING]
const KNIGHT_VS_BISHOP_CLOSED_BONUS: (i32, i32) = (20, 25);

/// Closed center threshold: minimum pawns on center 4 files to consider position closed.
/// 4 pawns on d/e files means both sides have pawns blocking each other.
const CLOSED_CENTER_PAWN_THRESHOLD: u32 = 4;

/// Prophylaxis: penalty for each available enemy pawn break.
/// A pawn break is an enemy pawn that can advance to challenge our pawn chain.
/// Karpov's #1 trait: prevent opponent's plans before they happen. [NEEDS TUNING]
const ENEMY_PAWN_BREAK_PENALTY: (i32, i32) = (-4, -2);

/// Piece coordination: bonus when our pieces mutually defend each other.
/// Karpov's pieces worked as a harmonious unit. [NEEDS TUNING]
const PIECE_COORDINATION_BONUS: (i32, i32) = (5, 0);

/// Piece coordination: bonus when multiple pieces control the same central square.
/// [NEEDS TUNING]
const CENTRAL_CONTROL_OVERLAP_BONUS: (i32, i32) = (1, 8);

// ---- NEW DATA-DRIVEN EVAL CONSTANTS ----
// Based on analysis of ~7K master games (Karpov, Petrosian, Keres)

/// Pieces in center: bonus per non-pawn piece on d4/d5/e4/e5.
/// Effect size +0.422 — one of the strongest positional predictors. [NEEDS TUNING]
const PIECE_IN_CENTER_BONUS: (i32, i32) = (0, 9);  // reduced: PST already rewards center

/// Piece-king proximity: penalty when a piece is far from our king.
/// Effect size -0.320 — pieces spread far from king correlate with losing. [NEEDS TUNING]
/// Applied per piece with chebyshev distance > 4 from king.
const PIECE_FAR_FROM_KING_PENALTY: (i32, i32) = (-2, 0);
const PIECE_KING_DISTANCE_THRESHOLD: u8 = 4;

/// Board quadrant spread: bonus for having pieces in multiple quadrants.
/// Effect size +0.479 — spreading pieces across the board = winning. [NEEDS TUNING]
/// Bonus per quadrant occupied (0-4 quadrants).
const QUADRANT_SPREAD_BONUS: (i32, i32) = (9, 11);

/// Extended center attack: bonus per square attacked in extended center (c3-f6).
/// Effect size +0.243 — controlling the extended center matters. [NEEDS TUNING]
const EXTENDED_CENTER_ATTACK_BONUS: (i32, i32) = (0, 5);

/// Advanced pawn bonus: extra reward for pawns past the 4th rank.
/// Effect size +0.328 — advanced pawns are strong. [NEEDS TUNING]
/// Per pawn on rank 5/6/7 (relative to color).
const ADVANCED_PAWN_BONUS_MG: [i32; 3] = [7, 15, 30];  // rank 5, 6, 7
const ADVANCED_PAWN_BONUS_EG: [i32; 3] = [2, 8, 35];

/// Min piece mobility: penalty when our least mobile piece has very few moves.
/// Effect size +0.171 — having a trapped/restricted piece is bad. [NEEDS TUNING]
const MIN_MOBILITY_PENALTY_THRESHOLD: u32 = 2;
const MIN_MOBILITY_PENALTY: (i32, i32) = (-12, -8);

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
        PieceType::Pawn   => PST_PAWN[idx],
        PieceType::Knight => PST_KNIGHT[idx],
        PieceType::Bishop => PST_BISHOP[idx],
        PieceType::Rook   => PST_ROOK[idx],
        PieceType::Queen  => PST_QUEEN[idx],
        PieceType::King   => PST_KING[idx],
        PieceType::None   => (0, 0),
    }
}

// ============================================================
// Section 2: Mobility tables
// ============================================================

const KNIGHT_MOBILITY: [(i32,i32); 9] = [
    (-30,-20),(-15,-10),(0,0),(5,5),(10,10),(15,15),(20,18),(25,20),(28,22),
];
const BISHOP_MOBILITY: [(i32,i32); 14] = [
    (-30,-25),(-15,-12),(0,0),(5,4),(8,7),(11,10),(14,13),(17,16),
    (19,18),(21,20),(23,22),(25,24),(26,25),(27,26),
];
const ROOK_MOBILITY: [(i32,i32); 15] = [
    (-25,-20),(-12,-10),(0,0),(3,3),(5,5),(7,7),(9,9),(11,11),
    (13,13),(15,15),(17,17),(19,19),(20,20),(21,21),(22,22),
];
const QUEEN_MOBILITY: [(i32,i32); 28] = [
    (-15,-10),(-8,-5),(0,0),(2,2),(4,4),(6,6),(8,8),(10,10),
    (12,12),(13,13),(14,14),(15,15),(16,16),(17,17),(18,18),(19,19),
    (20,20),(21,21),(21,21),(22,22),(22,22),(23,23),(23,23),(24,24),
    (24,24),(24,24),(25,25),(25,25),
];

// ============================================================
// Section 3: Main evaluation
// ============================================================

pub struct EvalContext<'a> {
    pub atk: &'a AttackTables,
}

/// Evaluate the position. Returns score in centipawns from the perspective of the side to move.
pub fn evaluate(board: &Board, ctx: &EvalContext) -> Score {
    let phase = compute_phase(board);

    let mut mg_score = 0i32;
    let mut eg_score = 0i32;

    let (mat_mg, mat_eg) = material_and_pst(board);
    mg_score += mat_mg;
    eg_score += mat_eg;

    let (mob_mg, mob_eg) = mobility_and_activity(board, ctx);
    mg_score += mob_mg;
    eg_score += mob_eg;

    let (pawn_mg, pawn_eg) = pawn_structure(board);
    mg_score += pawn_mg;
    eg_score += pawn_eg;

    let (ks_mg, ks_eg) = king_safety(board, ctx);
    mg_score += ks_mg;
    eg_score += ks_eg;

    let freedom = freedom_metric(board, ctx);
    mg_score += freedom;
    eg_score += freedom;

    // space_evaluation: disabled (-53 Elo in ablation)

    let (trade_mg, trade_eg) = trade_down_bonus(board);
    mg_score += trade_mg;
    eg_score += trade_eg;

    // bad_bishop_eval: disabled (-89 Elo in ablation, consistently hurts)

    let (ws_mg, ws_eg) = weak_square_eval(board, ctx);
    mg_score += ws_mg;
    eg_score += ws_eg;

    // knight_vs_bishop_eval: disabled (-53 Elo in ablation)

    // prophylaxis_eval: disabled (±0 Elo, data confirms near-zero effect)

    let (pc_mg, pc_eg) = piece_coordination_eval(board, ctx);
    mg_score += pc_mg;
    eg_score += pc_eg;


    let (ap_mg, ap_eg) = advanced_pawn_eval(board);
    mg_score += ap_mg;
    eg_score += ap_eg;

    // min_piece_mobility_eval: disabled (-127 Elo in ablation)

    let score = (mg_score * phase + eg_score * (256 - phase)) / 256;

    let tempo = TEMPO_BONUS;
    let side_sign = if board.side == Color::White { 1 } else { -1 };
    score * side_sign + tempo
}

fn compute_phase(board: &Board) -> i32 {
    let w = board.non_pawn_material(Color::White);
    let b = board.non_pawn_material(Color::Black);
    game_phase(w + b)
}

fn material_and_pst(board: &Board) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;
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
                mg += sign * (mat + pst_mg);
                eg += sign * (mat + pst_eg);
            }
        }
    }
    (mg, eg)
}

// ============================================================
// Section 4: Mobility and activity
// ============================================================
/// Rook file bonus: open file, semi-open file, or nothing.
fn rook_file_bonus(file_bb: Bb, our_pawns: Bb, their_pawns: Bb) -> (i32, i32) {
    if our_pawns & file_bb != 0 { return (0, 0); }
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
            let our_pawns   = board.pieces[ci][PieceType::Pawn as usize];
            let their_pawns = board.pieces[color.flip() as usize][PieceType::Pawn as usize];
            let (rk_mg, rk_eg) = rook_file_bonus(file_bb, our_pawns, their_pawns);
            mg += sign * rk_mg;
            eg += sign * rk_eg;

            let seventh_rank = if color == Color::White { BB_RANK_7 } else { BB_RANK_2 };
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
    if bb(sq) & their_pawn_attacks != 0 { return 0; }
    let r = sq_rank(sq);
    let in_outpost_zone = if color == Color::White {
        r >= 3 && r <= 5
    } else {
        r >= 2 && r <= 4
    };
    if !in_outpost_zone { return 0; }
    let our_pawn_attacks = if color == Color::White {
        pawn_attacks_white(board.pieces[color as usize][PieceType::Pawn as usize])
    } else {
        pawn_attacks_black(board.pieces[color as usize][PieceType::Pawn as usize])
    };
    if our_pawn_attacks & bb(sq) != 0 { OUTPOST_SUPPORTED } else { OUTPOST_UNSUPPORTED }
}

// ============================================================
// Section 5: The Karpov Freedom Metric
// ============================================================

/// Total pseudo-legal mobility for one side (pawns incl. pushes/captures,
/// pieces, king). The raw input to the squeeze bonus.
fn side_mobility(board: &Board, ctx: &EvalContext, color: Color) -> u32 {
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

/// Tiered squeeze bonus: restriction becomes exponentially more valuable
/// as the opponent's mobility approaches zero.
fn squeeze_bonus(mobility: u32) -> i32 {
    if mobility == 0 {
        SQUEEZE_TOTAL_LOCKDOWN
    } else if mobility <= 5 {
        SQUEEZE_SEVERE_BASE + (5 - mobility as i32) * SQUEEZE_SEVERE_PER_MOVE
    } else if mobility <= 15 {
        SQUEEZE_MODERATE_BASE + (15 - mobility as i32) * SQUEEZE_MODERATE_PER_MOVE
    } else if mobility <= 30 {
        (30 - mobility as i32).max(0)
    } else {
        0
    }
}

/// White-perspective freedom term, like every other eval term.
/// White is rewarded for restricting Black and penalized for being
/// restricted — symmetric, so the squeeze is scored correctly no matter
/// whose turn it is (the old stm-relative version inverted the bonus
/// whenever Black was to move).
fn freedom_metric(board: &Board, ctx: &EvalContext) -> i32 {
    let white_mob = side_mobility(board, ctx, Color::White);
    let black_mob = side_mobility(board, ctx, Color::Black);
    squeeze_bonus(black_mob) - squeeze_bonus(white_mob)
}

// ============================================================
// Section 5b: Space evaluation (Karpov signature)
// ============================================================
//
// Space = squares behind your pawn chain that you control.
// Karpov would gradually push pawns forward, claiming territory,
// then use the space advantage to maneuver pieces optimally.

fn space_evaluation(board: &Board) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;

    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let ci = color as usize;
        let our_pawns = board.pieces[ci][PieceType::Pawn as usize];
        let their_pawns = board.pieces[color.flip() as usize][PieceType::Pawn as usize];

        // Space behind our pawns: squares on our side of the pawn chain
        // that are not occupied by enemy pawns and are in center files
        let safe_zone = if color == Color::White {
            // White space = ranks 2-4 in center files, behind our most advanced pawn
            let behind = our_pawns | (our_pawns >> 8) | (our_pawns >> 16) | (our_pawns >> 24);
            behind & SPACE_CENTER_FILES & (BB_RANK_2 | BB_RANK_3 | BB_RANK_4)
        } else {
            let behind = our_pawns | (our_pawns << 8) | (our_pawns << 16) | (our_pawns << 24);
            behind & SPACE_CENTER_FILES & (BB_RANK_5 | BB_RANK_6 | BB_RANK_7)
        };

        // Count safe squares not blocked by enemy pawns
        let space_count = (safe_zone & !their_pawns).count_ones() as i32;
        mg += sign * space_count * SPACE_WEIGHT_MG;
        eg += sign * space_count * SPACE_WEIGHT_EG;
    }

    (mg, eg)
}

// ============================================================
// Section 5c: Trade-down bonus (Karpov endgame technique)
// ============================================================
//
// When ahead in material, it's advantageous to trade pieces (not pawns).
// This reduces the opponent's counterplay chances and makes the material
// advantage more decisive. Karpov was a master of this technique.

fn trade_down_bonus(board: &Board) -> (i32, i32) {
    let w_mat = board.non_pawn_material(Color::White);
    let b_mat = board.non_pawn_material(Color::Black);
    let mat_diff = w_mat - b_mat; // positive = white ahead

    if mat_diff.abs() < 100 { return (0, 0); }

    // How many pieces have been traded? Start = 5100, each trade reduces this.
    let total_npm = w_mat + b_mat;
    let pieces_traded = (5100 - total_npm).max(0);
    let trade_bonus = mat_diff.signum() * (mat_diff.abs() / 100) * TRADE_DOWN_BONUS_PER_100CP
                      * pieces_traded / 5100;

    // Primarily an endgame bonus
    (trade_bonus / 4, trade_bonus)
}

// ============================================================
// Section 5d: Bad bishop evaluation
// ============================================================
//
// A bishop is "bad" when many of its own pawns are on the same
// color squares, blocking its diagonals. Karpov excelled at
// exploiting bad bishops in his opponents' positions.

fn bad_bishop_eval(board: &Board) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;

    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let ci = color as usize;
        let our_pawns = board.pieces[ci][PieceType::Pawn as usize];
        let mut bishops = board.pieces[ci][PieceType::Bishop as usize];

        while bishops != 0 {
            let sq = bb_pop_lsb(&mut bishops);
            let bishop_color_mask = if bb(sq) & BB_LIGHT_SQUARES != 0 {
                BB_LIGHT_SQUARES
            } else {
                BB_DARK_SQUARES
            };
            // Count own pawns on same color squares as this bishop
            let blocking_pawns = (our_pawns & bishop_color_mask).count_ones() as i32;
            // Penalty scales with number of blocking pawns (3+ is bad)
            if blocking_pawns >= 3 {
                let excess = blocking_pawns - 2; // penalty for each pawn beyond 2
                mg += sign * excess * BAD_BISHOP_PENALTY_PER_PAWN.0;
                eg += sign * excess * BAD_BISHOP_PENALTY_PER_PAWN.1;
            }
        }
    }

    (mg, eg)
}


/// Build a bitboard mask of ranks ahead of `rank` for the given color, intersected with `file_mask`.
fn ranks_ahead(color: Color, rank: u8, file_mask: Bb) -> Bb {
    let mut mask = 0u64;
    if color == Color::White {
        for r in (rank + 1)..8 { mask |= BB_RANKS[r as usize]; }
    } else {
        for r in 0..rank { mask |= BB_RANKS[r as usize]; }
    }
    mask & file_mask
}

/// Build a bitboard mask of ranks behind (or equal to) `rank` for the given color, intersected with `file_mask`.
fn ranks_behind_inclusive(color: Color, rank: u8, file_mask: Bb) -> Bb {
    let mut mask = 0u64;
    if color == Color::White {
        for r in 0..=rank { mask |= BB_RANKS[r as usize]; }
    } else {
        for r in rank..8 { mask |= BB_RANKS[r as usize]; }
    }
    mask & file_mask
}

/// Evaluate a single passed pawn's bonuses (path clear, king proximity, connected, rook behind).
fn passed_pawn_bonuses(
    board: &Board, color: Color, sq: Square, rank: u8, file: u8,
    file_bb: Bb, adj_files: Bb, our_pawns: Bb, their_pawns: Bb, promo_dist: u8,
) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;
    let ci = color as usize;
    let ti = color.flip() as usize;
    let sign = if color == Color::White { 1 } else { -1 };
    let adv = (7 - promo_dist) as usize;

    mg += sign * PASSED_PAWN_BONUS_MG[adv.min(7)];
    eg += sign * PASSED_PAWN_BONUS_EG[adv.min(7)];

    // Path clear bonus
    let path_mask = ranks_ahead(color, rank, file_bb);
    if board.occ_all & path_mask == 0 {
        mg += sign * PASSER_PATH_CLEAR_BONUS.0;
        eg += sign * PASSER_PATH_CLEAR_BONUS.1;
    }

    // King proximity
    let our_king = board.king_sq[ci];
    let their_king = board.king_sq[ti];
    if our_king != NO_SQUARE && their_king != NO_SQUARE {
        let our_dist = chebyshev_distance(our_king, sq);
        let their_dist = chebyshev_distance(their_king, sq);
        eg += sign * (4i32 - our_dist as i32).max(0) * PASSER_KING_PROXIMITY_EG;
        eg += sign * (their_dist as i32 - 3).max(0) * PASSER_ENEMY_KING_DIST_EG;
    }

    // Connected passed pawns
    let mut adj_pawns = adj_files & our_pawns;
    while adj_pawns != 0 {
        let adj_sq = bb_pop_lsb(&mut adj_pawns);
        let adj_file_bb = BB_FILES[sq_file(adj_sq) as usize];
        let adj_rank = sq_rank(adj_sq);
        let adj_ahead = ranks_ahead(color, adj_rank, adj_file_bb | BB_FILES[file as usize]);
        if their_pawns & adj_ahead == 0 {
            mg += sign * CONNECTED_PASSER_BONUS.0;
            eg += sign * CONNECTED_PASSER_BONUS.1;
            break;
        }
    }

    // Rook behind passed pawn
    let rooks = board.pieces[ci][PieceType::Rook as usize];
    let behind_mask = ranks_behind_inclusive(color, rank, file_bb);
    if rooks & behind_mask != 0 {
        mg += sign * ROOK_BEHIND_PASSER_BONUS.0;
        eg += sign * ROOK_BEHIND_PASSER_BONUS.1;
    }

    (mg, eg)
}

/// Check if a pawn is backward: advance square attacked by enemy, no friendly support behind.
fn is_backward_pawn(
    color: Color, sq: Square, rank: u8, adj_files: Bb,
    our_pawns: Bb, their_pawn_attacks_bb: Bb,
) -> bool {
    // Must not be isolated (handled separately)
    if our_pawns & adj_files == 0 { return false; }

    let advance_sq = if color == Color::White {
        if rank >= 7 { return false; }
        sq + 8
    } else {
        if rank == 0 { return false; }
        sq - 8
    };

    // Advance square must be attacked by enemy pawn
    if bb(advance_sq) & their_pawn_attacks_bb == 0 { return false; }

    // No friendly pawn on adjacent files behind or equal rank can support
    let support_mask = ranks_behind_inclusive(color, rank, adj_files);
    our_pawns & support_mask == 0
}

/// Build defendable mask for weak square evaluation.
fn build_defendable_mask(color: Color, their_pawns: Bb) -> Bb {
    let mut defendable = 0u64;
    let mut tp = their_pawns;
    while tp != 0 {
        let sq = bb_pop_lsb(&mut tp);
        let f = sq_file(sq);
        let r = sq_rank(sq);
        let adj = if f > 0 { BB_FILES[(f-1) as usize] } else { 0 }
                | if f < 7 { BB_FILES[(f+1) as usize] } else { 0 };
        if color == Color::White {
            for dr in 0..r { defendable |= BB_RANKS[dr as usize] & adj; }
        } else {
            for dr in (r+1)..8 { defendable |= BB_RANKS[dr as usize] & adj; }
        }
    }
    defendable
}

/// Compute king proximity penalty for a single piece.
fn king_proximity_penalty(sq: Square, king_sq: Square) -> (i32, i32) {
    if king_sq == NO_SQUARE { return (0, 0); }
    let dist = chebyshev_distance(sq, king_sq);
    if dist <= PIECE_KING_DISTANCE_THRESHOLD { return (0, 0); }
    let excess = (dist - PIECE_KING_DISTANCE_THRESHOLD) as i32;
    (excess * PIECE_FAR_FROM_KING_PENALTY.0, excess * PIECE_FAR_FROM_KING_PENALTY.1)
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
        let our_pawns   = board.pieces[ci][PieceType::Pawn as usize];
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
            let left_file  = if file > 0 { BB_FILES[(file-1) as usize] } else { 0 };
            let right_file = if file < 7 { BB_FILES[(file+1) as usize] } else { 0 };
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
            let promo_dist = if color == Color::White { 7 - rank } else { rank };
            if their_pawns & ahead_mask == 0 && our_pawns & ranks_ahead(color, rank, file_bb) == 0 {
                let (pmg, peg) = passed_pawn_bonuses(
                    board, color, sq, rank, file, file_bb, adj_files,
                    our_pawns, their_pawns, promo_dist,
                );
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
        if king_sq == NO_SQUARE { continue; }

        // Pawn shield
        let king_file = sq_file(king_sq);
        let shield_rank = if color == Color::White {
            BB_RANK_2 | BB_RANK_3
        } else {
            BB_RANK_6 | BB_RANK_7
        };
        let shield_files = BB_FILES[king_file as usize]
            | (if king_file > 0 { BB_FILES[(king_file - 1) as usize] } else { 0 })
            | (if king_file < 7 { BB_FILES[(king_file + 1) as usize] } else { 0 });
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
            if ctx.atk.knight[sq as usize] & king_zone != 0 { attack_units += KING_ATTACK_WEIGHT_KNIGHT; }
        }
        let mut their_bishops = board.pieces[ti][PieceType::Bishop as usize];
        while their_bishops != 0 {
            let sq = bb_pop_lsb(&mut their_bishops);
            if ctx.atk.bishop_attacks(sq, occ) & king_zone != 0 { attack_units += KING_ATTACK_WEIGHT_BISHOP; }
        }
        let mut their_rooks = board.pieces[ti][PieceType::Rook as usize];
        while their_rooks != 0 {
            let sq = bb_pop_lsb(&mut their_rooks);
            if ctx.atk.rook_attacks(sq, occ) & king_zone != 0 { attack_units += KING_ATTACK_WEIGHT_ROOK; }
        }
        let mut their_queens = board.pieces[ti][PieceType::Queen as usize];
        while their_queens != 0 {
            let sq = bb_pop_lsb(&mut their_queens);
            if ctx.atk.queen_attacks(sq, occ) & king_zone != 0 { attack_units += KING_ATTACK_WEIGHT_QUEEN; }
        }

        let penalty = KING_SAFETY_TABLE.iter()
            .find(|&&(max_units, _)| attack_units <= max_units)
            .map(|&(_, p)| p)
            .unwrap_or(230);
        mg += sign * (-penalty);

        // King centralization bonus (endgame only)
        // Karpov's endgame mastery relied on active king placement
        let king_rank = sq_rank(king_sq) as i32;
        let center_dist = (3 - king_file as i32).abs().min((4 - king_file as i32).abs())
                        + (3 - king_rank).abs().min((4 - king_rank).abs());
        eg += sign * (3 - center_dist).max(0) * KING_CENTRALIZATION_EG;
    }

    (mg, eg)
}

// ============================================================
// Section 8: Weak square complex (Karpov exploitation)
// ============================================================
//
// A "weak square" (hole) is a square that can never be defended by
// enemy pawns because the adjacent file pawns have advanced past it
// or don't exist. Karpov would identify these holes and plant pieces
// on them, creating permanent positional advantages.

/// Chebyshev (king) distance between two squares
fn chebyshev_distance(a: Square, b: Square) -> u8 {
    let df = (sq_file(a) as i8 - sq_file(b) as i8).unsigned_abs();
    let dr = (sq_rank(a) as i8 - sq_rank(b) as i8).unsigned_abs();
    df.max(dr)
}

fn weak_square_eval(board: &Board, _ctx: &EvalContext) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;

    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let ci = color as usize;
        let ti = color.flip() as usize;
        let their_pawns = board.pieces[ti][PieceType::Pawn as usize];

        let outpost_ranks = if color == Color::White {
            BB_RANK_4 | BB_RANK_5 | BB_RANK_6
        } else {
            BB_RANK_3 | BB_RANK_4 | BB_RANK_5
        };

        let defendable = build_defendable_mask(color, their_pawns);
        let holes = outpost_ranks & !defendable;

        let our_knights = board.pieces[ci][PieceType::Knight as usize];
        let our_control = if color == Color::White {
            pawn_attacks_white(board.pieces[ci][PieceType::Pawn as usize])
        } else {
            pawn_attacks_black(board.pieces[ci][PieceType::Pawn as usize])
        };

        // Knights on holes
        let n_count = (our_knights & holes).count_ones() as i32;
        mg += sign * n_count * WEAK_SQUARE_KNIGHT_BONUS.0;
        eg += sign * n_count * WEAK_SQUARE_KNIGHT_BONUS.1;

        // General control of holes
        let c_count = (holes & our_control).count_ones() as i32;
        mg += sign * c_count * WEAK_SQUARE_CONTROL_BONUS.0;
        eg += sign * c_count * WEAK_SQUARE_CONTROL_BONUS.1;
    }

    (mg, eg)
}

// ============================================================
// Section 9: Good knight vs bad bishop (Karpov imbalance)
// ============================================================
//
// In closed positions, knights are superior to bishops because
// bishops need open diagonals. Karpov frequently engineered positions
// where he had the knight and his opponent had a bad bishop.

fn knight_vs_bishop_eval(board: &Board) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;

    // Count pawns on center files (d and e) to determine if position is closed
    let center_files = BB_FILES[3] | BB_FILES[4]; // d and e files
    let all_pawns = board.pieces[0][PieceType::Pawn as usize]
                  | board.pieces[1][PieceType::Pawn as usize];
    let center_pawns = (all_pawns & center_files).count_ones();

    if center_pawns < CLOSED_CENTER_PAWN_THRESHOLD { return (0, 0); }

    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let ci = color as usize;
        let ti = color.flip() as usize;

        let our_knights = board.pieces[ci][PieceType::Knight as usize].count_ones();
        let our_bishops = board.pieces[ci][PieceType::Bishop as usize].count_ones();
        let their_knights = board.pieces[ti][PieceType::Knight as usize].count_ones();
        let their_bishops = board.pieces[ti][PieceType::Bishop as usize].count_ones();

        // We have knight(s), no bishop; they have bishop(s), no knight
        if our_knights > 0 && our_bishops == 0 && their_bishops > 0 && their_knights == 0 {
            mg += sign * KNIGHT_VS_BISHOP_CLOSED_BONUS.0;
            eg += sign * KNIGHT_VS_BISHOP_CLOSED_BONUS.1;
        }
    }

    (mg, eg)
}

/// Count enemy pawn breaks: enemy pawns that can advance adjacent to our pawns.
fn count_pawn_breaks(color: Color, our_pawns: Bb, their_pawns: Bb, occ_all: Bb) -> i32 {
    let mut tp = their_pawns;
    let mut count = 0i32;
    while tp != 0 {
        let sq = bb_pop_lsb(&mut tp);
        let f = sq_file(sq);
        let adj = if f > 0 { BB_FILES[(f-1) as usize] } else { 0 }
                | if f < 7 { BB_FILES[(f+1) as usize] } else { 0 };
        if adj & our_pawns == 0 { continue; }

        // They are the opponent of `color`. If color is White, they are Black (advance = rank-1).
        let advance_sq = if color == Color::White {
            if sq_rank(sq) == 0 { continue; }
            sq - 8
        } else {
            if sq_rank(sq) == 7 { continue; }
            sq + 8
        };
        if occ_all & bb(advance_sq) == 0 { count += 1; }
    }
    count
}

// ============================================================
// Section 10: Prophylaxis evaluation (Karpov's #1 trait)
// ============================================================
//
// Prophylaxis = preventing opponent's plans. In evaluation terms,
// we penalize positions where the opponent has available pawn breaks
// (pawns that can advance to challenge our structure) and reward
// positions where those breaks have been prevented.

fn prophylaxis_eval(board: &Board) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;

    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let ci = color as usize;
        let ti = color.flip() as usize;
        let our_pawns = board.pieces[ci][PieceType::Pawn as usize];
        let their_pawns = board.pieces[ti][PieceType::Pawn as usize];

        let break_count = count_pawn_breaks(color, our_pawns, their_pawns, board.occ_all);
        mg += sign * break_count * ENEMY_PAWN_BREAK_PENALTY.0;
        eg += sign * break_count * ENEMY_PAWN_BREAK_PENALTY.1;
    }

    (mg, eg)
}

// ============================================================
// Section 11: Piece coordination & activity (combined)
// ============================================================
//
// Combines multiple data-driven signals into one cohesive term:
// - Mutual piece defense (pieces protecting each other)
// - Central control overlap (multiple piece types attacking extended center)
// - Piece centralization (non-pawn pieces on d4/d5/e4/e5) — small bonus, avoids PST overlap
// - Piece-king proximity (penalty for pieces far from king)
// - Board quadrant spread (bonus for flexible piece distribution)
// - Extended center attack count
//
// These are combined because they all measure "piece harmony and activity"
// and share the same piece-attack computation. Combining avoids redundant
// bitboard walks and keeps bonuses balanced.

fn piece_coordination_eval(board: &Board, ctx: &EvalContext) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;

    // Quadrant masks (computed once)
    let q_masks: [Bb; 4] = [
        (BB_FILE_A | BB_FILE_B | BB_FILE_C | BB_FILE_D) & (BB_RANK_1 | BB_RANK_2 | BB_RANK_3 | BB_RANK_4),
        (BB_FILE_E | BB_FILE_F | BB_FILE_G | BB_FILE_H) & (BB_RANK_1 | BB_RANK_2 | BB_RANK_3 | BB_RANK_4),
        (BB_FILE_A | BB_FILE_B | BB_FILE_C | BB_FILE_D) & (BB_RANK_5 | BB_RANK_6 | BB_RANK_7 | BB_RANK_8),
        (BB_FILE_E | BB_FILE_F | BB_FILE_G | BB_FILE_H) & (BB_RANK_5 | BB_RANK_6 | BB_RANK_7 | BB_RANK_8),
    ];

    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let ci = color as usize;
        let occ = board.occ_all;
        let our_occ = board.occ[ci];
        let king_sq = board.king_sq[ci];

        let our_non_pawn_non_king = our_occ
            & !board.pieces[ci][PieceType::Pawn as usize]
            & !board.pieces[ci][PieceType::King as usize];

        let mut piece_attacks: [Bb; 4] = [0; 4]; // N, B, R, Q aggregate attacks
        let mut total_attacks = 0u64;

        // Knights
        let mut knights = board.pieces[ci][PieceType::Knight as usize];
        while knights != 0 {
            let sq = bb_pop_lsb(&mut knights);
            let atk = ctx.atk.knight[sq as usize];
            piece_attacks[0] |= atk;
            total_attacks |= atk;
            let (kp_mg, kp_eg) = king_proximity_penalty(sq, king_sq);
            mg += sign * kp_mg;
            eg += sign * kp_eg;
        }

        // Bishops
        let mut bishops = board.pieces[ci][PieceType::Bishop as usize];
        while bishops != 0 {
            let sq = bb_pop_lsb(&mut bishops);
            let atk = ctx.atk.bishop_attacks(sq, occ);
            piece_attacks[1] |= atk;
            total_attacks |= atk;
            let (kp_mg, kp_eg) = king_proximity_penalty(sq, king_sq);
            mg += sign * kp_mg;
            eg += sign * kp_eg;
        }

        // Rooks
        let mut rooks = board.pieces[ci][PieceType::Rook as usize];
        while rooks != 0 {
            let sq = bb_pop_lsb(&mut rooks);
            let atk = ctx.atk.rook_attacks(sq, occ);
            piece_attacks[2] |= atk;
            total_attacks |= atk;
            let (kp_mg, kp_eg) = king_proximity_penalty(sq, king_sq);
            mg += sign * kp_mg;
            eg += sign * kp_eg;
        }

        // Queens (skip king proximity — long-range pieces)
        let mut queens = board.pieces[ci][PieceType::Queen as usize];
        while queens != 0 {
            let sq = bb_pop_lsb(&mut queens);
            let atk = ctx.atk.queen_attacks(sq, occ);
            piece_attacks[3] |= atk;
            total_attacks |= atk;
        }

        // 1. Mutual defense
        let defended_minors_majors = (total_attacks & our_occ)
            & !board.pieces[ci][PieceType::Pawn as usize]
            & !board.pieces[ci][PieceType::King as usize];
        let def_count = defended_minors_majors.count_ones() as i32;
        mg += sign * def_count * PIECE_COORDINATION_BONUS.0;
        eg += sign * def_count * PIECE_COORDINATION_BONUS.1;

        // 2. Central control overlap
        let center = BB_EXTENDED_CENTER;
        let mut overlap_count = 0i32;
        for i in 0..4 {
            for j in (i+1)..4 {
                overlap_count += (piece_attacks[i] & piece_attacks[j] & center).count_ones() as i32;
            }
        }
        mg += sign * overlap_count * CENTRAL_CONTROL_OVERLAP_BONUS.0;
        eg += sign * overlap_count * CENTRAL_CONTROL_OVERLAP_BONUS.1;

        // 3. Piece centralization
        let center_pieces = (our_non_pawn_non_king & BB_CENTER).count_ones() as i32;
        mg += sign * center_pieces * PIECE_IN_CENTER_BONUS.0;
        eg += sign * center_pieces * PIECE_IN_CENTER_BONUS.1;

        // 4. Extended center attack
        let ext_center_attacks = (total_attacks & BB_EXTENDED_CENTER).count_ones() as i32;
        mg += sign * ext_center_attacks * EXTENDED_CENTER_ATTACK_BONUS.0;
        eg += sign * ext_center_attacks * EXTENDED_CENTER_ATTACK_BONUS.1;

        // 5. Quadrant spread
        let mut quadrants_occupied = 0i32;
        for &qm in &q_masks {
            if our_non_pawn_non_king & qm != 0 { quadrants_occupied += 1; }
        }
        let spread_bonus = (quadrants_occupied - 1).max(0);
        mg += sign * spread_bonus * QUADRANT_SPREAD_BONUS.0;
        eg += sign * spread_bonus * QUADRANT_SPREAD_BONUS.1;
    }

    (mg, eg)
}


// ============================================================
// Section 18: Advanced pawns (data-driven, effect +0.328)
// ============================================================
//
// Pawns that have pushed past the 4th rank get an extra bonus
// beyond what PST provides. Advanced pawns restrict the opponent
// and create outpost support.

fn advanced_pawn_eval(board: &Board) -> (i32, i32) {
    let mut mg = 0i32;
    let mut eg = 0i32;

    for &color in &[Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let ci = color as usize;
        let our_pawns = board.pieces[ci][PieceType::Pawn as usize];

        // Rank 5, 6, 7 for white; Rank 4, 3, 2 for black
        let (r5, r6, r7) = if color == Color::White {
            (BB_RANK_5, BB_RANK_6, BB_RANK_7)
        } else {
            (BB_RANK_4, BB_RANK_3, BB_RANK_2)
        };

        let on_5 = (our_pawns & r5).count_ones() as i32;
        let on_6 = (our_pawns & r6).count_ones() as i32;
        let on_7 = (our_pawns & r7).count_ones() as i32;

        mg += sign * (on_5 * ADVANCED_PAWN_BONUS_MG[0] + on_6 * ADVANCED_PAWN_BONUS_MG[1] + on_7 * ADVANCED_PAWN_BONUS_MG[2]);
        eg += sign * (on_5 * ADVANCED_PAWN_BONUS_EG[0] + on_6 * ADVANCED_PAWN_BONUS_EG[1] + on_7 * ADVANCED_PAWN_BONUS_EG[2]);
    }

    (mg, eg)
}

// ============================================================
// Section 19: Min piece mobility (data-driven, effect +0.171)
// ============================================================
//
// A penalty when our least mobile piece has very few moves.
// Having a trapped or nearly trapped piece is a major liability.

fn min_piece_mobility_eval(board: &Board, ctx: &EvalContext) -> (i32, i32) {
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

        let mut min_mob = u32::MAX;
        let mut has_piece = false;

        // Knights
        let mut knights = board.pieces[ci][PieceType::Knight as usize];
        while knights != 0 {
            has_piece = true;
            let sq = bb_pop_lsb(&mut knights);
            let mob = (ctx.atk.knight[sq as usize] & !our_occ & !their_pawn_attacks).count_ones();
            min_mob = min_mob.min(mob);
        }

        // Bishops
        let mut bishops = board.pieces[ci][PieceType::Bishop as usize];
        while bishops != 0 {
            has_piece = true;
            let sq = bb_pop_lsb(&mut bishops);
            let mob = (ctx.atk.bishop_attacks(sq, occ) & !our_occ & !their_pawn_attacks).count_ones();
            min_mob = min_mob.min(mob);
        }

        // Rooks
        let mut rooks = board.pieces[ci][PieceType::Rook as usize];
        while rooks != 0 {
            has_piece = true;
            let sq = bb_pop_lsb(&mut rooks);
            let mob = (ctx.atk.rook_attacks(sq, occ) & !our_occ).count_ones();
            min_mob = min_mob.min(mob);
        }

        // Queens
        let mut queens = board.pieces[ci][PieceType::Queen as usize];
        while queens != 0 {
            has_piece = true;
            let sq = bb_pop_lsb(&mut queens);
            let mob = (ctx.atk.queen_attacks(sq, occ) & !our_occ & !their_pawn_attacks).count_ones();
            min_mob = min_mob.min(mob);
        }

        if has_piece && min_mob <= MIN_MOBILITY_PENALTY_THRESHOLD {
            mg += sign * MIN_MOBILITY_PENALTY.0;
            eg += sign * MIN_MOBILITY_PENALTY.1;
        }
    }

    (mg, eg)
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
        let adj_files = (if file > 0 { BB_FILES[(file - 1) as usize] } else { 0 })
            | (if file < 7 { BB_FILES[(file + 1) as usize] } else { 0 });
        let promo_dist = if color == Color::White { 7 - rank } else { rank };

        passed_pawn_bonuses(
            &board,
            color,
            sq,
            rank,
            file,
            file_bb,
            adj_files,
            board.pieces[ci][PieceType::Pawn as usize],
            board.pieces[color.flip() as usize][PieceType::Pawn as usize],
            promo_dist,
        )
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
