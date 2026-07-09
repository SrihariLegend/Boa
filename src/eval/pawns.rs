use super::*;
pub(in crate::eval) fn ranks_ahead(color: Color, rank: u8, file_mask: Bb) -> Bb {
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
pub(in crate::eval) fn ranks_behind_inclusive(color: Color, rank: u8, file_mask: Bb) -> Bb {
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

pub(in crate::eval) struct PassedPawnContext {
    pub(in crate::eval) sq: Square,
    pub(in crate::eval) rank: u8,
    pub(in crate::eval) file: u8,
    pub(in crate::eval) file_bb: Bb,
    pub(in crate::eval) adj_files: Bb,
    pub(in crate::eval) our_pawns: Bb,
    pub(in crate::eval) their_pawns: Bb,
    pub(in crate::eval) promo_dist: u8,
}

/// Evaluate a single passed pawn's bonuses (path clear, king proximity, connected, rook behind).
pub(in crate::eval) fn passed_pawn_bonuses(
    board: &Board,
    color: Color,
    passed: PassedPawnContext,
) -> (i32, i32) {
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
pub(in crate::eval) fn is_backward_pawn(
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

    // No friendly pawn on adjacent files strictly behind (pawns defend
    // diagonally forward, not sideways — same-rank pawns don't count).
    let support_mask = ranks_behind_inclusive(color, rank, adj_files) & !BB_RANKS[rank as usize];
    our_pawns & support_mask == 0
}

// ============================================================
// Section 6: Pawn structure
// ============================================================

pub(in crate::eval) fn pawn_structure(board: &Board) -> (i32, i32) {
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
// Pawn evaluation cache
// ============================================================

const PAWN_CACHE_SIZE: usize = 1024;

/// Direct-mapped cache for `pawn_structure()` results, keyed by `board.pawn_hash`.
/// Pawn structures change rarely during search so hit rates are very high (~95%+).
pub struct PawnEvalCache {
    entries: Box<[(u64, (i32, i32)); PAWN_CACHE_SIZE]>,
}

impl PawnEvalCache {
    pub fn new() -> Self {
        Self {
            entries: Box::new([(0u64, (0i32, 0i32)); PAWN_CACHE_SIZE]),
        }
    }

    pub fn probe(&self, key: u64) -> Option<(i32, i32)> {
        let idx = (key as usize) & (PAWN_CACHE_SIZE - 1);
        let (stored_key, score) = self.entries[idx];
        if stored_key == key {
            Some(score)
        } else {
            None
        }
    }

    pub fn store(&mut self, key: u64, mg: i32, eg: i32) {
        let idx = (key as usize) & (PAWN_CACHE_SIZE - 1);
        self.entries[idx] = (key, (mg, eg));
    }
}
