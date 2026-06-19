// ============================================================
// tt.rs - Transposition table
// ============================================================

use crate::types::{Move, Score, MAX_PLY, SCORE_MATE};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Bound {
    None = 0,
    Exact = 1,
    Lower = 2,
    Upper = 3,
}

#[derive(Clone, Copy)]
pub struct TtEntry {
    pub key: u32,
    pub score: i32,
    pub best: Move,
    pub depth: i8,
    pub bound: Bound,
    pub age: u8,
}

const CTRL_BUSY: u64 = 1u64 << 63;

struct AtomicTtSlot {
    ctrl: AtomicU64,
    data: AtomicU64,
}

impl AtomicTtSlot {
    fn empty() -> Self {
        AtomicTtSlot {
            ctrl: AtomicU64::new(0),
            data: AtomicU64::new(0),
        }
    }
}

pub struct TranspositionTable {
    entries: Vec<AtomicTtSlot>,
    mask: usize,
    age: AtomicU8,
}

impl TranspositionTable {
    pub fn new(size_mb: usize) -> Self {
        let entry_size = std::mem::size_of::<AtomicTtSlot>();
        let count = (size_mb * 1024 * 1024 / entry_size).next_power_of_two();
        let mask = count - 1;
        TranspositionTable {
            entries: std::iter::repeat_with(AtomicTtSlot::empty)
                .take(count)
                .collect(),
            mask,
            age: AtomicU8::new(0),
        }
    }

    #[inline(always)]
    fn index(&self, hash: u64) -> usize {
        (hash as usize) & self.mask
    }

    pub fn probe(&self, hash: u64) -> Option<TtEntry> {
        let slot = &self.entries[self.index(hash)];
        let ctrl = slot.ctrl.load(Ordering::Acquire);
        if ctrl == 0 || ctrl & CTRL_BUSY != 0 {
            return None;
        }
        let data = slot.data.load(Ordering::Acquire);
        if ctrl != slot.ctrl.load(Ordering::Acquire) {
            return None;
        }
        let entry = unpack_entry(ctrl, data);
        if entry.bound != Bound::None && entry.key == (hash >> 32) as u32 {
            Some(entry)
        } else {
            None
        }
    }

    pub fn store(&self, hash: u64, score: Score, best: Move, depth: i8, bound: Bound) {
        let slot = &self.entries[self.index(hash)];
        let key = (hash >> 32) as u32;
        let age = self.age.load(Ordering::Relaxed);
        let ctrl = slot.ctrl.load(Ordering::Acquire);

        if ctrl != 0 && ctrl & CTRL_BUSY == 0 {
            let current = unpack_entry(ctrl, slot.data.load(Ordering::Relaxed));
            if current.key != key && current.age == age && depth < current.depth {
                return;
            }
        }

        slot.ctrl.store(CTRL_BUSY, Ordering::Release);
        slot.data.store(pack_data(score, best), Ordering::Release);
        slot.ctrl
            .store(pack_ctrl(key, depth, bound, age), Ordering::Release);
    }

    pub fn new_search(&self) {
        self.age.fetch_add(1, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        for slot in &self.entries {
            slot.ctrl.store(0, Ordering::Relaxed);
            slot.data.store(0, Ordering::Relaxed);
        }
        self.age.store(0, Ordering::Relaxed);
    }

    pub fn hashfull(&self) -> usize {
        let age = self.age.load(Ordering::Relaxed);
        self.entries
            .iter()
            .take(1000)
            .filter(|slot| {
                let ctrl = slot.ctrl.load(Ordering::Relaxed);
                if ctrl == 0 || ctrl & CTRL_BUSY != 0 {
                    return false;
                }
                let entry = unpack_entry(ctrl, slot.data.load(Ordering::Relaxed));
                entry.age == age && entry.bound != Bound::None
            })
            .count()
    }
}

fn pack_ctrl(key: u32, depth: i8, bound: Bound, age: u8) -> u64 {
    (key as u64)
        | ((depth as u8 as u64) << 32)
        | ((bound as u8 as u64) << 40)
        | ((age as u64) << 48)
}

fn pack_data(score: Score, best: Move) -> u64 {
    (score as u32 as u64) | ((best as u64) << 32)
}

fn unpack_entry(ctrl: u64, data: u64) -> TtEntry {
    let bound = match ((ctrl >> 40) & 0xFF) as u8 {
        1 => Bound::Exact,
        2 => Bound::Lower,
        3 => Bound::Upper,
        _ => Bound::None,
    };
    TtEntry {
        key: ctrl as u32,
        score: data as u32 as i32,
        best: (data >> 32) as Move,
        depth: ((ctrl >> 32) & 0xFF) as u8 as i8,
        bound,
        age: ((ctrl >> 48) & 0xFF) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tt_round_trips_entry() {
        let tt = TranspositionTable::new(1);
        let hash = 0x1234_5678_9abc_def0;

        tt.new_search();
        tt.store(hash, -123, 0x4321, 7, Bound::Lower);

        let entry = tt.probe(hash).expect("stored entry");
        assert_eq!(entry.key, 0x1234_5678);
        assert_eq!(entry.score, -123);
        assert_eq!(entry.best, 0x4321);
        assert_eq!(entry.depth, 7);
        assert_eq!(entry.bound, Bound::Lower);
    }
}
