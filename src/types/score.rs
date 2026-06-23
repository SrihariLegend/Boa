// ---- Score / evaluation ----

pub type Score = i32;
pub const SCORE_INF: Score = 1_000_000;
pub const SCORE_MATE: Score = 900_000;
pub const SCORE_DRAW: Score = 0;

/// Returns true if the score is a mate score
/// Max search depth (plies). Mate scores encode ply, so this bounds the range.
pub const MAX_PLY: usize = 128;

pub fn is_mate_score(s: Score) -> bool {
    s.abs() >= SCORE_MATE - MAX_PLY as Score
}

/// Converts a mate score to mate-in-N (positive = we mate, negative = they mate)
pub fn mate_in(s: Score) -> i32 {
    if s > 0 {
        (SCORE_MATE - s + 1) / 2
    } else {
        -(SCORE_MATE + s + 1) / 2
    }
}
