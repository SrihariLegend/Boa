use super::*;
/// Mate scores are stored relative to the *node* (distance to mate from here),
/// not the root - otherwise the same position reached at different plies would
/// cache contradictory scores. Convert root-relative -> node-relative on store.
pub fn score_to_tt(s: Score, ply: usize) -> Score {
    if s >= SCORE_MATE - MAX_PLY as Score {
        s + ply as Score
    } else if s <= -(SCORE_MATE - MAX_PLY as Score) {
        s - ply as Score
    } else {
        s
    }
}

/// Convert node-relative mate score back to root-relative on probe.
pub fn score_from_tt(s: Score, ply: usize) -> Score {
    if s >= SCORE_MATE - MAX_PLY as Score {
        s - ply as Score
    } else if s <= -(SCORE_MATE - MAX_PLY as Score) {
        s + ply as Score
    } else {
        s
    }
}
