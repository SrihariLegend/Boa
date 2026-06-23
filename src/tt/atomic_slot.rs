use super::*;
pub(super) const CTRL_BUSY: u64 = 1u64 << 63;

pub(super) struct AtomicTtSlot {
    pub(super) ctrl: AtomicU64,
    pub(super) data: AtomicU64,
}

impl AtomicTtSlot {
    pub(super) fn empty() -> Self {
        AtomicTtSlot {
            ctrl: AtomicU64::new(0),
            data: AtomicU64::new(0),
        }
    }
}
