use super::*;

#[test]
pub(super) fn tt_round_trips_entry() {
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
