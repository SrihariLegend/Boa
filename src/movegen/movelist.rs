use super::*;
// ============================================================
// Section 3: Move list
// ============================================================

pub struct MoveList {
    pub moves: [Move; 256],
    pub scores: [i32; 256],
    pub count: usize,
}

impl MoveList {
    pub fn new() -> Self {
        MoveList {
            moves: [0; 256],
            scores: [0; 256],
            count: 0,
        }
    }

    #[inline(always)]
    pub fn push(&mut self, m: Move) {
        self.moves[self.count] = m;
        self.scores[self.count] = 0;
        self.count += 1;
    }

    pub fn iter(&self) -> &[Move] {
        &self.moves[..self.count]
    }

    /// Partial sort: bring best-scored move to front. O(n) per call.
    pub fn pick_best(&mut self, start: usize) {
        let mut best_idx = start;
        for i in (start + 1)..self.count {
            if self.scores[i] > self.scores[best_idx] {
                best_idx = i;
            }
        }
        self.moves.swap(start, best_idx);
        self.scores.swap(start, best_idx);
    }
}
