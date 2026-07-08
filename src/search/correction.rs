use super::*;
use crate::board::{Board, Zobrist};

// ---- Correction history constants ----

/// Maximum absolute correction value stored in any table entry.
const CORRHIST_LIMIT: i32 = 1024;

/// Divisor for correction value before applying to eval.
/// correction_value / CORRHIST_DIVISOR is the actual centipawn adjustment.

/// Gravity constant for correction history updates.
const CORRHIST_GRAVITY: i32 = 1024;

/// Number of pawn correction buckets.
const PAWN_CORR_SIZE: usize = 16384;

/// Number of non-pawn correction buckets.
const NONPAWN_CORR_SIZE: usize = 16384;

/// Continuation correction dimension (piece*64 + to → max 6*64 = 384).
const CONT_CORR_SIZE: usize = 384;

/// Correction weights — reduced 10× from initial plan values.
/// Original values (30/35/27) produced average corrections of 68 cp,
/// flipping 40% of RFP decisions and causing -69 Elo regression.
const CORR_W1: i32 = 3;
const CORR_W2: i32 = 3;
const CORR_W3: i32 = 3;

// ---- Non-pawn hash computation ----

/// Compute a Zobrist hash of non-pawn pieces for a single color.
/// Used to key the non-pawn correction tables.
pub(in crate::search) fn non_pawn_hash(board: &Board, z: &Zobrist, color: Color) -> u64 {
    let mut h: u64 = 0;
    let ci = color as usize;
    for pt in [
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ] {
        let mut bb = board.pieces[ci][pt as usize];
        while bb != 0 {
            let sq = bb_lsb(bb);
            bb &= bb - 1;
            h ^= z.piece_sq[ci][pt as usize][sq as usize];
        }
    }
    h
}

// ---- Correction computation ----

/// Compute the correction value for the current position.
/// Called once per node before any pruning decision.
/// The corrected eval = raw_eval + correction / CORRHIST_DIVISOR.
pub(in crate::search) fn compute_correction(ctx: &SearchContext, board: &Board, ply: usize) -> i32 {
    let stm = board.side as usize;
    let pawn_idx = (board.pawn_hash as usize) % PAWN_CORR_SIZE;

    // Use cached non-pawn hashes if available (avoid recomputing twice per node).
    let (np_idx_w, np_idx_b) = if ply < MAX_PLY {
        if let Some((hw, hb)) = ctx.stack[ply].non_pawn_hashes {
            (
                hw as usize % NONPAWN_CORR_SIZE,
                hb as usize % NONPAWN_CORR_SIZE,
            )
        } else {
            let np_hash_w = non_pawn_hash(board, ctx.z, Color::White);
            let np_hash_b = non_pawn_hash(board, ctx.z, Color::Black);
            (
                np_hash_w as usize % NONPAWN_CORR_SIZE,
                np_hash_b as usize % NONPAWN_CORR_SIZE,
            )
        }
    } else {
        let np_hash_w = non_pawn_hash(board, ctx.z, Color::White);
        let np_hash_b = non_pawn_hash(board, ctx.z, Color::Black);
        (
            np_hash_w as usize % NONPAWN_CORR_SIZE,
            np_hash_b as usize % NONPAWN_CORR_SIZE,
        )
    };

    let mut corr = CORR_W1 * ctx.pawn_corr[stm][pawn_idx]
        + CORR_W2 * ctx.nonpawn_corr_w[stm][np_idx_w]
        + CORR_W2 * ctx.nonpawn_corr_b[stm][np_idx_b];

    if ply >= 2 {
        if let (Some(prev1), Some(prev2)) =
            (ctx.stack[ply - 1].cont_entry, ctx.stack[ply - 2].cont_entry)
        {
            let cont_idx = prev1.0 * 64 + prev1.1;
            let cont2_idx = prev2.0 * 64 + prev2.1;
            if cont_idx < CONT_CORR_SIZE && cont2_idx < CONT_CORR_SIZE {
                corr += CORR_W3 * ctx.cont_corr[stm][cont_idx][cont2_idx];
            }
        }
    }

    corr
}

/// Apply correction to raw_eval and return the corrected eval.
/// The raw_eval is the uncorrected static evaluation.
/// The corrected eval is what feeds into all pruning margins.

// ---- Correction history update ----

/// Update all correction history tables after search returns from a node.
/// Uses the difference between the search result and the raw_eval to learn
/// systematic eval biases for this position type.
///
/// Must be called ONCE per node, AFTER the search returns best_score.
/// The update uses best_score - raw_eval (NOT best_score - corrected_eval) —
/// the correction learns the total eval error, not the residual after correction.
pub(in crate::search) fn update_correction(
    ctx: &mut SearchContext,
    board: &Board,
    depth: i32,
    best_score: Score,
    raw_eval: Score,
    ply: usize,
) {
    // Only update at depth ≥ 4. Shallow-depth search results (d ≤ 3) are
    // dominated by tactical noise and would poison the correction tables.
    if depth < 4 {
        return;
    }

    let diff = best_score - raw_eval;
    if diff.abs() < 5 {
        return; // negligible error, skip update to avoid noise
    }

    // Skip mate scores — they would inject massive spikes into the correction
    // tables for position types that share hash buckets with unrelated positions.
    if is_mate_score(best_score) || is_mate_score(raw_eval) {
        return;
    }

    let bonus = (diff * depth / 4).clamp(-CORRHIST_LIMIT / 4, CORRHIST_LIMIT / 4);

    let stm = board.side as usize;
    let pawn_idx = (board.pawn_hash as usize) % PAWN_CORR_SIZE;

    // Pawn correction
    {
        let old = ctx.pawn_corr[stm][pawn_idx];
        ctx.pawn_corr[stm][pawn_idx] = old + bonus - (old * bonus.abs()) / CORRHIST_GRAVITY;
    }

    // Non-pawn correction (both colors) — use cached hashes if available.
    let (np_idx_w, np_idx_b) = if ply < MAX_PLY {
        if let Some((hw, hb)) = ctx.stack[ply].non_pawn_hashes {
            (
                hw as usize % NONPAWN_CORR_SIZE,
                hb as usize % NONPAWN_CORR_SIZE,
            )
        } else {
            let np_hash_w = non_pawn_hash(board, ctx.z, Color::White);
            let np_hash_b = non_pawn_hash(board, ctx.z, Color::Black);
            (
                np_hash_w as usize % NONPAWN_CORR_SIZE,
                np_hash_b as usize % NONPAWN_CORR_SIZE,
            )
        }
    } else {
        let np_hash_w = non_pawn_hash(board, ctx.z, Color::White);
        let np_hash_b = non_pawn_hash(board, ctx.z, Color::Black);
        (
            np_hash_w as usize % NONPAWN_CORR_SIZE,
            np_hash_b as usize % NONPAWN_CORR_SIZE,
        )
    };

    {
        let old = ctx.nonpawn_corr_w[stm][np_idx_w];
        ctx.nonpawn_corr_w[stm][np_idx_w] = old + bonus - (old * bonus.abs()) / CORRHIST_GRAVITY;
    }
    {
        let old = ctx.nonpawn_corr_b[stm][np_idx_b];
        ctx.nonpawn_corr_b[stm][np_idx_b] = old + bonus - (old * bonus.abs()) / CORRHIST_GRAVITY;
    }

    // Continuation correction: (prev1, prev2) and (prev1, prev4) pairs
    if ply >= 2 {
        if let (Some(prev1), Some(prev2)) =
            (ctx.stack[ply - 1].cont_entry, ctx.stack[ply - 2].cont_entry)
        {
            let cont_idx = prev1.0 * 64 + prev1.1;
            let cont2_idx = prev2.0 * 64 + prev2.1;
            if cont_idx < CONT_CORR_SIZE && cont2_idx < CONT_CORR_SIZE {
                let old = ctx.cont_corr[stm][cont_idx][cont2_idx];
                ctx.cont_corr[stm][cont_idx][cont2_idx] =
                    old + bonus - (old * bonus.abs()) / CORRHIST_GRAVITY;
            }
        }
    }
    if ply >= 4 {
        if let Some(prev4) = ctx.stack[ply - 4].cont_entry {
            let cont_idx = prev4.0 * 64 + prev4.1;
            if let Some(prev1) = ctx.stack[ply - 1].cont_entry {
                let cont1_idx = prev1.0 * 64 + prev1.1;
                if cont1_idx < CONT_CORR_SIZE && cont_idx < CONT_CORR_SIZE {
                    let old = ctx.cont_corr[stm][cont1_idx][cont_idx];
                    ctx.cont_corr[stm][cont1_idx][cont_idx] =
                        old + bonus - (old * bonus.abs()) / CORRHIST_GRAVITY;
                }
            }
        }
    }
}
