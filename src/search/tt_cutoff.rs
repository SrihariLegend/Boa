use super::*;
use crate::probe;

pub(in crate::search) fn try_tt_cutoff(
    ctx: &mut SearchContext,
    hash: u64,
    depth: i32,
    alpha: Score,
    beta: Score,
    is_pv: bool,
    ply: usize,
) -> (Move, Option<Score>, Option<i16>) {
    ctx.stats.tt_probes += 1;
    let entry = match ctx.tt.probe(hash) {
        Some(e) => e,
        None => return (MOVE_NONE, None, None),
    };
    ctx.stats.tt_hits += 1;
    let tt_move = entry.best;
    let tt_raw_eval = entry.raw_eval;

    if is_pv || entry.depth < depth as i8 {
        return (tt_move, None, Some(tt_raw_eval));
    }

    let s = score_from_tt(entry.score, ply);
    let cutoff = match entry.bound {
        Bound::Exact => true,
        Bound::Lower => s >= beta,
        Bound::Upper => s <= alpha,
        _ => false,
    };
    let et = match entry.bound {
        Bound::Exact => "exact",
        Bound::Lower => "lower",
        Bound::Upper => "upper",
        _ => "none",
    };
    probe!(
        TtCutoff,
        TtCutoffEvent {
            depth: depth,
            entry_type: et,
            entry_depth: entry.depth,
            depth_sufficient: entry.depth >= depth as i8,
            cutoff_score: s,
            alpha: alpha,
            beta: beta,
        }
    );
    if cutoff {
        ctx.stats.tt_cutoffs += 1;
        return (tt_move, Some(s), Some(tt_raw_eval));
    }
    (tt_move, None, Some(tt_raw_eval))
}
