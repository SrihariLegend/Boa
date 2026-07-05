use super::*;
use crate::sample_probe;
pub struct TranspositionTable {
    entries: Vec<AtomicTtSlot>,
    mask: usize,
    age: AtomicU8,
    size_mb: usize,
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
            size_mb,
        }
    }

    pub fn size_mb(&self) -> usize {
        self.size_mb
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
            sample_probe!(
                4,
                TtProbe,
                TtProbeEvent {
                    operation: "probe",
                    hit: true,
                    entry_type: bound_str(entry.bound),
                    entry_depth: entry.depth,
                    entry_score: entry.score,
                    entry_age: entry.age,
                    slot_index: self.index(hash) as u8,
                    replaced: false,
                    replaced_depth: 0,
                }
            );
            Some(entry)
        } else {
            sample_probe!(
                16,
                TtProbe,
                TtProbeEvent {
                    operation: "probe",
                    hit: false,
                    entry_type: "empty",
                    entry_depth: 0,
                    entry_score: 0,
                    entry_age: 0,
                    slot_index: self.index(hash) as u8,
                    replaced: false,
                    replaced_depth: 0,
                }
            );
            None
        }
    }

    pub fn store(
        &self,
        hash: u64,
        score: Score,
        best: Move,
        depth: i8,
        bound: Bound,
        raw_eval: i16,
    ) {
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
        slot.data
            .store(pack_data(score, best, raw_eval), Ordering::Release);
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
