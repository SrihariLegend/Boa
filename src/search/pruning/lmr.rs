use super::super::*;

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

    let pre_protection_reduction = reduction.clamp(0, input.depth - 2);
    if pre_protection_reduction > 0
        && criticality_score(input, base_reduction, pre_protection_reduction)
            >= CRITICALITY_P97_LOGIT
    {
        reduction -= 1;
    }

    LmrReduction {
        base_reduction,
        final_reduction: reduction.clamp(0, input.depth - 2),
    }
}

pub(in crate::search) fn criticality_score(
    input: LmrInput,
    base_reduction: i32,
    final_reduction: i32,
) -> f64 {
    let new_depth = if final_reduction > 0 {
        (input.depth - 1 - final_reduction).max(1)
    } else {
        input.depth - 1
    };
    let prev_static_eval = input.prev_static_eval.unwrap_or(0);
    let static_eval_delta = input
        .prev_static_eval
        .map_or(0, |prev| input.static_eval - prev);
    let piece = if input.moving_piece == PIECE_NONE {
        PieceType::None
    } else {
        piece_type(input.moving_piece)
    };

    CRITICALITY_INTERCEPT
        - 0.152_875_012_601_427_25 * (input.root_depth as f64 / 16.0)
        + 0.735_838_646_984_717_2 * (input.ply as f64 / 32.0)
        - 0.873_714_182_579_038_2 * (input.depth as f64 / 16.0)
        - 1.043_699_758_610_700_6 * (input.move_index as f64 / 32.0)
        - 0.513_712_172_623_182_1 * (base_reduction as f64 / 4.0)
        + 1.544_485_099_792_215 * (final_reduction as f64 / 4.0)
        - 2.159_049_968_790_647_3 * (new_depth as f64 / 16.0)
        + 1.556_016_650_597_994_2 * normalized_history(input.history_score)
        + 5.739_198_384_264_096 * normalized_score(input.static_eval)
        - 0.316_867_213_444_681_1 * bool_feature(input.prev_static_eval.is_some())
        + 1.901_766_259_089_970_9 * normalized_score(prev_static_eval)
        + 4.004_276_897_688_178 * normalized_score(static_eval_delta)
        - 4.994_272_043_215_292 * normalized_score(input.alpha)
        - 0.382_227_234_880_739_06 * normalized_score(input.beta)
        + 0.584_084_076_062_692_6 * bool_feature(input.is_pv)
        - 0.584_084_076_062_692_7 * bool_feature(input.is_cut_node)
        + 0.447_596_124_223_620_94 * bool_feature(input.improving)
        - 1.947_485_685_342_310_6 * bool_feature(input.is_counter)
        - 0.011_544_318_219_468_804 * bool_feature(input.side_to_move == Color::Black)
        + 0.295_647_661_544_235_83 * bool_feature(piece == PieceType::Pawn)
        - 0.108_058_810_201_738_88 * bool_feature(piece == PieceType::Knight)
        - 0.059_180_402_269_422_84 * bool_feature(piece == PieceType::Bishop)
        - 0.123_707_657_637_899_55 * bool_feature(piece == PieceType::Rook)
        - 0.295_075_323_807_634_35 * bool_feature(piece == PieceType::Queen)
        + 0.130_156_508_673_690_16 * bool_feature(piece == PieceType::King)
        // The trained weights for is_killer and tt_move_agreement are exactly zero.
        + 0.0 * bool_feature(input.is_killer)
        + 0.0 * bool_feature(input.tt_move_agreement)
}

pub(in crate::search) fn normalized_score(score: Score) -> f64 {
    score.clamp(-2_000, 2_000) as f64 / 2_000.0
}

pub(in crate::search) fn normalized_history(history_score: i32) -> f64 {
    history_score.clamp(-16_384, 16_384) as f64 / 16_384.0
}

pub(in crate::search) fn bool_feature(value: bool) -> f64 {
    if value {
        1.0
    } else {
        0.0
    }
}
