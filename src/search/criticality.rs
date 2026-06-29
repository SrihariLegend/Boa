use super::*;
pub(in crate::search) fn build_criticality_record(
    ctx: &SearchContext,
    input: CriticalityRecordInput,
) -> Option<CriticalityRecord> {
    if !input.enabled || ctx.criticality_logger.is_none() {
        return None;
    }
    Some(CriticalityRecord {
        decision_kind: CriticalityDecisionKind::Lmr,
        pid: std::process::id(),
        game_id: ctx.game_id,
        search_id: ctx.search_id,
        root_depth: ctx.root_depth,
        ply: input.ply,
        node_hash: input.node_hash,
        side_to_move: input.side_to_move,
        m: input.m,
        from: input.from,
        to: input.to,
        piece: input.moving_piece,
        depth: input.depth,
        move_index: input.move_index,
        base_reduction: input.base_reduction,
        final_reduction: input.final_reduction,
        new_depth: input.new_depth,
        history_score: input.history_score,
        static_eval: input.static_eval,
        prev_static_eval: input.prev_static_eval,
        alpha: input.alpha,
        beta: input.beta,
        futility_margin: None,
        static_alpha_margin: None,
        is_pv: input.is_pv,
        is_cut_node: input.is_cut_node,
        improving: input.improving,
        is_killer: input.is_killer,
        is_counter: input.is_counter,
        tt_move_agreement: input.tt_move_agreement,
        label_source: CriticalityLabelSource::None,
        reduced_score: None,
        full_score: None,
        sigma: input.sigma,
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::search) fn should_run_criticality_probe(
    ctx: &SearchContext,
    node_hash: u64,
    m: Move,
    depth: i32,
    ply: usize,
    reduction: i32,
    reduced_score: Score,
    alpha: Score,
) -> bool {
    reduction > 0
        && !ctx.in_criticality_probe
        && reduced_score <= alpha
        && ctx.criticality_logger.is_some()
        && should_probe_criticality(
            node_hash,
            m,
            depth,
            ply,
            ctx.search_id,
            ctx.options.criticality.probe_permille,
        )
}

pub(in crate::search) fn should_run_futility_probe(
    ctx: &SearchContext,
    node_hash: u64,
    m: Move,
    depth: i32,
    ply: usize,
) -> bool {
    !ctx.in_criticality_probe
        && ctx.criticality_logger.is_some()
        && should_probe_criticality(
            node_hash,
            m,
            depth,
            ply,
            ctx.search_id,
            ctx.options.criticality.futility_probe_permille,
        )
}

pub(in crate::search) fn write_criticality_record(
    ctx: &mut SearchContext,
    record: &CriticalityRecord,
) {
    let Some(logger) = &mut ctx.criticality_logger else {
        return;
    };
    if let Err(err) = logger.write(record) {
        eprintln!("info string criticality log write failed: {err}");
        ctx.criticality_logger = None;
    }
}
