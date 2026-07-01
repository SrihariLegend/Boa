use super::*;

mod ffp;
mod lmr;
mod rfp;

pub(in crate::search) use ffp::*;
pub(in crate::search) use lmr::*;
pub(in crate::search) use rfp::*;
pub use ffp::ffp_margin;

pub(in crate::search) fn is_improving(ctx: &SearchContext, static_eval: Score, ply: usize) -> bool {
    if ply < 2 || ply - 2 >= MAX_PLY {
        return false;
    }
    ctx.stack[ply - 2]
        .static_eval
        .is_some_and(|prev_eval| static_eval > prev_eval)
}
