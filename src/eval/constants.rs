// ---- Evaluation tuning constants ----
// Sources: Stockfish (SF), Chess Programming Wiki (CPW), or marked [NEEDS TUNING].
//
// Piece values are defined in types.rs (Kaufman values: P=100, N=320, B=330, R=500, Q=900).

/// Bishop pair bonus (mg, eg). Having two bishops is worth extra material.
/// SF uses ~30/50 (tuned). CPW recommends 25-50. [NEEDS TUNING]
pub(in crate::eval) const BISHOP_PAIR_BONUS: (i32, i32) = (30, 50);

/// Rook on fully open file (no pawns of either color). (mg, eg)
/// SF: ~20/7. CPW: 15-25. [NEEDS TUNING]
pub(in crate::eval) const ROOK_OPEN_FILE_BONUS: (i32, i32) = (20, 10);

/// Rook on semi-open file (no friendly pawns). (mg, eg)
/// SF: ~7/6. CPW: 8-15. [NEEDS TUNING]
pub(in crate::eval) const ROOK_SEMI_OPEN_FILE_BONUS: (i32, i32) = (10, 5);

/// Rook on 7th rank bonus. Strong in both phases.
/// SF: ~15-30 depending on context. CPW: 20-30. [NEEDS TUNING]
pub(in crate::eval) const ROOK_ON_SEVENTH_BONUS: (i32, i32) = (20, 30);

/// Knight outpost bonus when supported/unsupported by own pawn.
/// An outpost is a square on ranks 4-6 not attackable by enemy pawns.
/// SF: ~30-50 for supported outposts. These are conservative. [NEEDS TUNING]
/// (Values were swapped — a supported outpost must outscore an unsupported one.)
pub(in crate::eval) const OUTPOST_SUPPORTED: i32 = 20;
pub(in crate::eval) const OUTPOST_UNSUPPORTED: i32 = 10;

/// Tempo bonus: side-to-move advantage in centipawns.
/// SF uses ~28 (tuned). 15 is conservative. [NEEDS TUNING]
pub(in crate::eval) const TEMPO_BONUS: i32 = 10;

/// Doubled pawn penalty (mg, eg). Per-pawn: each pawn on a doubled file
/// incurs this penalty. A doubled pair costs 2×, a tripled group costs 3×.
/// SF: ~-5/-20 (file-dependent). CPW: -10 to -20. [NEEDS TUNING]
pub(in crate::eval) const DOUBLED_PAWN_PENALTY: (i32, i32) = (-5, -10);

/// Isolated pawn penalty (mg, eg). No friendly pawns on adjacent files.
/// SF: ~-10/-20. CPW: -15 to -25. [NEEDS TUNING]
pub(in crate::eval) const ISOLATED_PAWN_PENALTY: (i32, i32) = (-10, -20);

/// Backward pawn penalty (mg, eg). Pawn on starting rank with no adjacent support.
/// Less studied than isolated. SF has complex backward pawn logic. [NEEDS TUNING]
pub(in crate::eval) const BACKWARD_PAWN_PENALTY: (i32, i32) = (-8, -12);

/// Pawn chain bonus per protected pawn (mg, eg).
/// Pawns defending each other. SF: ~3-5. [NEEDS TUNING]
pub(in crate::eval) const PAWN_CHAIN_BONUS: (i32, i32) = (3, 5);

/// Passed pawn bonus tables indexed by advancement (distance from promotion).
/// Values increase exponentially as pawn advances. Shape follows SF/CPW convention. [NEEDS TUNING]
pub(in crate::eval) const PASSED_PAWN_BONUS_MG: [i32; 8] = [0, 0, 5, 10, 20, 40, 70, 0];
pub(in crate::eval) const PASSED_PAWN_BONUS_EG: [i32; 8] = [0, 5, 10, 20, 40, 80, 120, 0];

/// Pawn shield: bonus per shielding pawn, with a base penalty for exposed king.
/// shield_score = count * PER_PAWN - BASE_PENALTY
/// With 3 shield pawns: 3*10 - 30 = 0 (neutral). 0 pawns: -30. [NEEDS TUNING]
pub(in crate::eval) const PAWN_SHIELD_PER_PAWN: i32 = 10;
pub(in crate::eval) const PAWN_SHIELD_BASE_PENALTY: i32 = 30;

/// King zone attack unit weights by piece type.
/// Each piece attacking the king zone contributes this many "attack units".
/// Inspired by CPW safety tables. Queens count most, pawns/kings not counted. [NEEDS TUNING]
pub(in crate::eval) const KING_ATTACK_WEIGHT_KNIGHT: i32 = 2;
pub(in crate::eval) const KING_ATTACK_WEIGHT_BISHOP: i32 = 2;
pub(in crate::eval) const KING_ATTACK_WEIGHT_ROOK: i32 = 3;
pub(in crate::eval) const KING_ATTACK_WEIGHT_QUEEN: i32 = 5;

/// King safety penalty table: maps attack_units to penalty.
/// Loosely follows the CPW safety table shape (quadratic-ish growth). [NEEDS TUNING]
pub(in crate::eval) const KING_SAFETY_TABLE: [(i32, i32); 7] = [
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
pub(in crate::eval) const ROOK_BEHIND_PASSER_BONUS: (i32, i32) = (10, 20);

/// King centralization in endgame: bonus per rank/file closer to center. [NEEDS TUNING]
pub(in crate::eval) const KING_CENTRALIZATION_EG: i32 = 10;

/// Connected passed pawn bonus multiplier.
/// Two passed pawns on adjacent files supporting each other. [NEEDS TUNING]
pub(in crate::eval) const CONNECTED_PASSER_BONUS: (i32, i32) = (10, 20);

/// Passed pawn path clear bonus: extra bonus when no piece blocks the passer's path. [NEEDS TUNING]
pub(in crate::eval) const PASSER_PATH_CLEAR_BONUS: (i32, i32) = (5, 20);

/// Passed pawn king proximity bonus: bonus when friendly king is near the passer.
/// Scale: per rank of proximity (closer = more bonus). Endgame only. [NEEDS TUNING]
pub(in crate::eval) const PASSER_KING_PROXIMITY_EG: i32 = 5;

/// Passed pawn enemy king distance bonus: bonus when enemy king is far from passer.
/// Endgame only, per rank of distance. [NEEDS TUNING]
pub(in crate::eval) const PASSER_ENEMY_KING_DIST_EG: i32 = 5;
