use super::*;
pub(super) fn board_to_shakmaty(board: &Board) -> Option<Chess> {
    board
        .to_fen()
        .parse::<Fen>()
        .ok()?
        .into_position(CastlingMode::Standard)
        .ok()
}

pub(super) fn probe_position_score(
    tables: &Tablebase<Chess>,
    pos: &Chess,
    options: &SyzygyOptions,
    ply: usize,
) -> Option<Score> {
    if options.fifty_move_rule {
        return tables
            .probe_wdl(pos)
            .ok()
            .map(|wdl| ambiguous_wdl_score(wdl, ply));
    }
    tables
        .probe_wdl_after_zeroing(pos)
        .ok()
        .map(|wdl| wdl_score(wdl, ply))
}

pub(super) fn wdl_score(wdl: Wdl, ply: usize) -> Score {
    match wdl {
        Wdl::Win => TB_WIN_SCORE - ply as Score,
        Wdl::CursedWin => TB_CURSED_WIN_SCORE,
        Wdl::Draw => SCORE_DRAW,
        Wdl::BlessedLoss => -TB_CURSED_WIN_SCORE,
        Wdl::Loss => -TB_WIN_SCORE + ply as Score,
    }
}

pub(super) fn ambiguous_wdl_score(wdl: AmbiguousWdl, ply: usize) -> Score {
    if let Some(unambiguous) = wdl.unambiguous() {
        return wdl_score(unambiguous, ply);
    }
    match wdl.signum() {
        1 => TB_CURSED_WIN_SCORE,
        -1 => -TB_CURSED_WIN_SCORE,
        _ => SCORE_DRAW,
    }
}

pub(super) fn score_wdl_name(score: Score) -> &'static str {
    if score >= TB_WIN_SCORE / 2 {
        "win"
    } else if score <= -TB_WIN_SCORE / 2 {
        "loss"
    } else if score > 0 {
        "cursed-win"
    } else if score < 0 {
        "blessed-loss"
    } else {
        "draw"
    }
}
