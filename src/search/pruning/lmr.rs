// ============================================================
// lmr.rs — Classical Late Move Reductions
//
// Standard formula: log(depth) × log(moves) / divisor with
// history adjustment, PV/cut-node scaling, and improving bonus.
// ============================================================

use super::super::*;
use crate::sample_probe;

pub(in crate::search) fn compute_lmr_reduction_details(
    input: LmrInput,
    ctx: &mut SearchContext,
) -> LmrReduction {
    if input.moves_searched < LMR_FULL_DEPTH_MOVES
        || input.depth < LMR_MIN_DEPTH
        || input.is_capture
        || input.is_promo
        || input.gives_check
        || input.in_check
    {
        return LmrReduction {
            base_reduction: 0,
            final_reduction: 0,
        };
    }
    ctx.stats.lmr_attempts += 1;
    let move_count = input.moves_searched - LMR_FULL_DEPTH_MOVES + 1;
    let depth_ln = (input.depth as f64).ln();
    let move_ln = (move_count as f64).ln();
    let mut reduction = (0.5 + depth_ln * move_ln / LMR_LOG_DIVISOR).floor() as i32;
    let base_reduction = reduction;

    let history_bonus =
        input.history_score.max(0).clamp(0, LMR_HISTORY_CLAMP) / LMR_HISTORY_NORMALIZER;
    reduction -= history_bonus;

    if LMR_NODE_TYPE_SCALING {
        if input.is_pv {
            reduction = (reduction * 3 + 3) / 4;
        } else if input.is_cut_node {
            reduction = (reduction * 23 + 10) / 20;
        }
    }

    if input.improving {
        reduction += LMR_IMPROVING_BONUS;
    }

    let final_reduction = reduction.clamp(0, input.depth - 2);
    let new_depth = if final_reduction > 0 {
        (input.depth - 1 - final_reduction).max(1)
    } else {
        input.depth - 1
    };

    sample_probe!(4, Lmr, LmrEvent {
        depth: input.depth,
        ply: input.ply as u32,
        move_index: input.move_index as u32,
        moves_searched: input.moves_searched as u32,
        history_score: input.history_score,
        base_reduction: base_reduction,
        actual_reduction: final_reduction,
        new_depth: new_depth,
        improving: input.improving,
        is_killer: input.is_killer,
        is_counter: input.is_counter,
        tt_move_agreement: input.tt_move_agreement,
        gives_check: input.gives_check,
        moving_piece: input.moving_piece as u8,
        is_cut_node: input.is_cut_node,
    });

    LmrReduction {
        base_reduction,
        final_reduction,
    }
}
