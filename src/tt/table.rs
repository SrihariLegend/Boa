use super::*;
use crate::probe;

pub struct Bucket {
    entries: [AtomicTtSlot; 3],
}

impl Bucket {
    pub fn empty() -> Self {
        Bucket {
            entries: [
                AtomicTtSlot::empty(),
                AtomicTtSlot::empty(),
                AtomicTtSlot::empty(),
            ],
        }
    }
}

pub struct TranspositionTable {
    buckets: Box<[Bucket]>,
    _mask: usize,
    num_buckets: usize,
    age: AtomicU16,
    size_mb: usize,
}

impl TranspositionTable {
    pub fn new(size_mb: usize) -> Self {
        let entry_size = std::mem::size_of::<Bucket>();
        let num_buckets = (size_mb * 1024 * 1024 / entry_size).next_power_of_two();
        let mask = num_buckets - 1;
        TranspositionTable {
            buckets: std::iter::repeat_with(Bucket::empty)
                .take(num_buckets)
                .collect::<Vec<Bucket>>()
                .into_boxed_slice(),
            _mask: mask,
            num_buckets,
            age: AtomicU16::new(0),
            size_mb,
        }
    }

    pub fn size_mb(&self) -> usize {
        self.size_mb
    }

    #[inline(always)]
    fn index(&self, hash: u64) -> usize {
        ((hash as u128 * self.num_buckets as u128) >> 64) as usize
    }

    pub fn probe(&self, hash: u64) -> Option<TtEntry> {
        let bucket = &self.buckets[self.index(hash)];
        let key = hash as u32;

        for (i, slot) in bucket.entries.iter().enumerate() {
            #[allow(unused_variables)]
            let _ = i;
            let ctrl = slot.ctrl.load(Ordering::Acquire);
            if ctrl == 0 || ctrl & CTRL_BUSY != 0 {
                continue;
            }
            let data = slot.data.load(Ordering::Acquire);
            if ctrl != slot.ctrl.load(Ordering::Acquire) {
                // ABA problem, retry or skip
                continue;
            }
            let entry = unpack_entry(ctrl, data);
            if entry.key == key {
                probe!(
                    TtProbe,
                    TtProbeEvent {
                        operation: "probe",
                        hit: true,
                        entry_type: bound_str(entry.bound),
                        entry_depth: entry.depth,
                        entry_score: entry.score,
                        entry_age: entry.age,
                        slot_index: i as u8,
                        replaced: false,
                        replaced_depth: 0,
                    }
                );
                return Some(entry);
            }
        }
        probe!(
            TtProbe,
            TtProbeEvent {
                operation: "probe",
                hit: false,
                entry_type: "empty",
                entry_depth: 0,
                entry_score: 0,
                entry_age: 0,
                slot_index: 0, // No specific slot hit
                replaced: false,
                replaced_depth: 0,
            }
        );
        None
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
        let bucket = &self.buckets[self.index(hash)];
        let key = hash as u32;
        let age = self.age.load(Ordering::Relaxed);

        let mut _best_quality = i32::MAX;
        let mut replace_slot_index = 0;
        let mut _replaced_depth = 0;

        // Try to find a matching entry or the weakest entry for replacement
        for (i, slot) in bucket.entries.iter().enumerate() {
            #[allow(unused_variables)]
            let _ = i;
            let ctrl = slot.ctrl.load(Ordering::Acquire);
            if ctrl & CTRL_BUSY != 0 {
                // Slot is busy, cannot use it
                continue;
            }

            if ctrl != 0 {
                let current = unpack_entry(ctrl, slot.data.load(Ordering::Relaxed));
                if current.key == key {
                    // Matching key found: update this entry
                    if depth < current.depth {
                        // Don't overwrite if new depth is shallower, even across searches.
                        // High depth naturally protects the entry via the quality score.
                        probe!(
                            TtProbe,
                            TtProbeEvent {
                                operation: "store",
                                hit: true,
                                entry_type: bound_str(current.bound),
                                entry_depth: current.depth,
                                entry_score: current.score,
                                entry_age: current.age,
                                slot_index: i as u8,
                                replaced: false,
                                replaced_depth: 0,
                            }
                        );
                        return;
                    }
                    // This slot will be updated
                    _replaced_depth = current.depth;
                    replace_slot_index = i;
                    _best_quality = -i32::MAX; // Force update this slot
                    break;
                }

                // Calculate quality for replacement.
                // Depth-0 (qsearch) entries are always preferred for eviction
                // when the incoming entry has depth > 0 — qsearch entries only
                // help other qsearch nodes and should never block main-search data.
                let age_distance = (age.wrapping_sub(current.age)) as i32;
                let quality = if current.depth == 0 && depth > 0 {
                    -i32::MAX + 1 // evict before any main-search entry, but after empty slots
                } else {
                    current.depth as i32 - 4 * age_distance
                };
                if quality < _best_quality {
                    _best_quality = quality;
                    replace_slot_index = i;
                    _replaced_depth = current.depth;
                }
            } else {
                // Empty slot, best possible quality for replacement
                replace_slot_index = i;
                _replaced_depth = 0;
                _best_quality = -i32::MAX;
                break;
            }
        }

        if _best_quality == i32::MAX {
            // All slots are busy, skip store
            return;
        }

        // Perform the store operation on the chosen slot
        let slot_to_update = &bucket.entries[replace_slot_index];

        let mut current_ctrl = slot_to_update.ctrl.load(Ordering::Relaxed);
        loop {
            if current_ctrl & CTRL_BUSY != 0 {
                return; // Another thread is writing, just drop our store
            }
            match slot_to_update.ctrl.compare_exchange_weak(
                current_ctrl,
                CTRL_BUSY,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(c) => current_ctrl = c,
            }
        }

        slot_to_update
            .data
            .store(pack_data(score, best, raw_eval), Ordering::Release);
        slot_to_update
            .ctrl
            .store(pack_ctrl(key, depth, bound, age), Ordering::Release); // Release lock

        probe!(
            TtProbe,
            TtProbeEvent {
                operation: "store",
                hit: true, // "hit" means the store operation was performed
                entry_type: bound_str(bound),
                entry_depth: depth,
                entry_score: score.into(),
                entry_age: age,
                slot_index: replace_slot_index as u8,
                replaced: true,
                replaced_depth: _replaced_depth,
            }
        );
    }

    pub fn new_search(&self) {
        self.age.fetch_add(1, Ordering::Relaxed);
    }

    /// Clear all TT entries. Caller must ensure no search threads are running
    /// (i.e., the stop flag has been set and all workers have joined). Clearing
    /// while a concurrent `store()` holds a BUSY lock loses the lock bit and
    /// leaves a partially written entry in the cleared slot.
    pub fn clear(&self) {
        for bucket in self.buckets.iter() {
            for slot in bucket.entries.iter() {
                slot.ctrl.store(0, Ordering::Relaxed);
                slot.data.store(0, Ordering::Relaxed);
            }
        }
        self.age.store(0, Ordering::Relaxed);
    }

    pub fn hashfull(&self) -> usize {
        let age = self.age.load(Ordering::Relaxed);
        let count = self.buckets
            .iter()
            .take(1000) // Sample first 1000 buckets
            .flat_map(|bucket| bucket.entries.iter())
            .filter(|slot| {
                let ctrl = slot.ctrl.load(Ordering::Relaxed);
                if ctrl == 0 || ctrl & CTRL_BUSY != 0 {
                    return false;
                }
                let entry = unpack_entry(ctrl, slot.data.load(Ordering::Relaxed));
                entry.age == age && entry.bound != Bound::None
            })
            .count();
        
        // Spec 14.4: Each bucket has 3 entries. Count all 3, sum across all 1000 buckets.
        // Return sum * 1000 / 3000 to produce a value in 0-1000 (permille) standard UCI format.
        (count * 1000) / 3000
    }
}
