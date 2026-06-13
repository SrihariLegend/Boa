// ============================================================
// tt.rs — Transposition table
// ============================================================

#![allow(dead_code)]

use crate::types::{Move, Score, MOVE_NONE, SCORE_MATE, MAX_PLY};

/// Mate scores are stored relative to the *node* (distance to mate from here),
/// not the root — otherwise the same position reached at different plies would
/// cache contradictory scores. Convert root-relative → node-relative on store.
pub fn score_to_tt(s: Score, ply: usize) -> Score {
    if s >= SCORE_MATE - MAX_PLY as Score { s + ply as Score }
    else if s <= -(SCORE_MATE - MAX_PLY as Score) { s - ply as Score }
    else { s }
}

/// Convert node-relative mate score back to root-relative on probe.
pub fn score_from_tt(s: Score, ply: usize) -> Score {
    if s >= SCORE_MATE - MAX_PLY as Score { s - ply as Score }
    else if s <= -(SCORE_MATE - MAX_PLY as Score) { s + ply as Score }
    else { s }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Bound {
    None  = 0,
    Exact = 1,
    Lower = 2,
    Upper = 3,
}

#[derive(Clone, Copy)]
pub struct TtEntry {
    pub key:   u32,
    pub score: i32, // i32 so mate scores (±900K) survive — i16 clamping destroyed them
    pub best:  Move,
    pub depth: i8,
    pub bound: Bound,
    pub age:   u8,
}

impl TtEntry {
    const fn empty() -> Self {
        TtEntry {
            key: 0, score: 0, best: MOVE_NONE,
            depth: 0, bound: Bound::None, age: 0,
        }
    }
    // Entry stays 16 bytes after padding (4+4+4+1+1+1 → 16), same as with i16.
}

pub struct TranspositionTable {
    entries: Vec<TtEntry>,
    mask:    usize,
    pub age: u8,
}

impl TranspositionTable {
    pub fn new(size_mb: usize) -> Self {
        let entry_size = std::mem::size_of::<TtEntry>();
        let count = (size_mb * 1024 * 1024 / entry_size).next_power_of_two();
        let mask  = count - 1;
        TranspositionTable {
            entries: vec![TtEntry::empty(); count],
            mask,
            age: 0,
        }
    }

    #[inline(always)]
    fn index(&self, hash: u64) -> usize {
        (hash as usize) & self.mask
    }

    pub fn probe(&self, hash: u64) -> Option<&TtEntry> {
        let idx = self.index(hash);
        let e = &self.entries[idx];
        if e.bound != Bound::None && e.key == (hash >> 32) as u32 {
            Some(e)
        } else {
            None
        }
    }

    pub fn store(&mut self, hash: u64, score: Score, best: Move, depth: i8, bound: Bound) {
        let idx = self.index(hash);
        let key = (hash >> 32) as u32;
        let e   = &mut self.entries[idx];

        if e.key == key || e.age != self.age || depth >= e.depth {
            e.key   = key;
            e.score = score;
            e.best  = best;
            e.depth = depth;
            e.bound = bound;
            e.age   = self.age;
        }
    }

    pub fn new_search(&mut self) {
        self.age = self.age.wrapping_add(1);
    }

    pub fn clear(&mut self) {
        for e in &mut self.entries { *e = TtEntry::empty(); }
        self.age = 0;
    }

    pub fn hashfull(&self) -> usize {
        let sample = self.entries.iter().take(1000);
        let used = sample.filter(|e| e.age == self.age && e.bound != Bound::None).count();
        used
    }
}
