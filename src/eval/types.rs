use super::*;
use crate::sample_probe;
use std::cell::RefCell;
// ============================================================
// Section 3: Main evaluation
// ============================================================

pub struct EvalContext<'a> {
    pub atk: &'a AttackTables,
    pub options: &'a EngineOptions,
    pub pawn_cache: &'a RefCell<PawnEvalCache>,
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

    let (pawn_mg, pawn_eg) = {
        let probe_result = ctx.pawn_cache.borrow().probe(board.pawn_hash);
        if let Some(cached) = probe_result {
            cached
        } else {
            let result = pawn_structure(board);
            ctx.pawn_cache.borrow_mut().store(board.pawn_hash, result.0, result.1);
            result
        }
    };
    let (pawn_mg, pawn_eg) =
        scale_score_pair((pawn_mg, pawn_eg), ctx.options.eval.pawn_structure_scale);
    mg_score += pawn_mg;
    eg_score += pawn_eg;

    let (ks_mg, ks_eg) = king_safety(board, ctx);
    let (ks_mg, ks_eg) = scale_score_pair((ks_mg, ks_eg), ctx.options.eval.king_safety_scale);
    mg_score += ks_mg;
    eg_score += ks_eg;

    let score = blend_phase(mg_score, eg_score, phase);

    let tempo = TEMPO_BONUS;
    let side_sign = if board.side == Color::White { 1 } else { -1 };
    let result = EvalBreakdown {
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
        white_score: score,
        side_to_move_score: score * side_sign + tempo,
    };

    sample_probe!(
        16,
        Eval,
        EvalEvent {
            phase: phase,
            material_mg: mat_mg,
            material_eg: mat_eg,
            material_cp: result.material_cp,
            pst_mg: pst_mg,
            pst_eg: pst_eg,
            pst_cp: result.pst_cp,
            mobility_mg: mob_mg,
            mobility_eg: mob_eg,
            mobility_cp: result.mobility_cp,
            mobility_white: mobility_white,
            mobility_black: mobility_black,
            pawn_structure_mg: pawn_mg,
            pawn_structure_eg: pawn_eg,
            pawn_structure_cp: result.pawn_structure_cp,
            king_safety_mg: ks_mg,
            king_safety_eg: ks_eg,
            king_safety_cp: result.king_safety_cp,
            white_score: score,
            side_to_move_score: result.side_to_move_score,
        }
    );

    result
}

pub(in crate::eval) fn blend_phase(mg: i32, eg: i32, phase: i32) -> i32 {
    (mg * phase + eg * (256 - phase)) / 256
}
