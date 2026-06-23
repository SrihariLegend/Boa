use super::*;
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
