use super::*;

#[test]
pub(super) fn tt_round_trips_entry() {
    let tt = TranspositionTable::new(1);
    let hash = 0x1234_5678_9abc_def0;

    tt.new_search();
    tt.store(hash, -123, 0x4321, 7, Bound::Lower, 0);

    let entry = tt.probe(hash).expect("stored entry");
    assert_eq!(entry.key, 0x9abc_def0);
    assert_eq!(entry.score, -123);
    assert_eq!(entry.best, 0x4321);
    assert_eq!(entry.depth, 7);
    assert_eq!(entry.bound, Bound::Lower);
}

#[test]
pub(super) fn tt_round_trips_raw_eval() {
    let tt = TranspositionTable::new(1);
    let hash = 0xABCD_EF01_2345_6789;

    tt.new_search();
    tt.store(hash, 42, 0x1234, 5, Bound::Exact, -150);

    let entry = tt.probe(hash).expect("stored entry");
    assert_eq!(entry.raw_eval, -150);
    assert_eq!(entry.score, 42);
    assert_eq!(entry.best, 0x1234);
}

#[test]
pub(super) fn tt_raw_eval_zero_round_trips() {
    let tt = TranspositionTable::new(1);
    let hash = 0xDEAD_BEEF_CAFE_BABE;

    tt.new_search();
    tt.store(hash, -500, 0xABCD, 3, Bound::Upper, 0);

    let entry = tt.probe(hash).expect("stored entry");
    assert_eq!(entry.raw_eval, 0);
}

#[test]
pub(super) fn tt_raw_eval_boundary_values() {
    let tt = TranspositionTable::new(1);
    tt.new_search();

    // Use hashes with non-zero upper 32 bits so the stored key ≠ 0
    tt.store(0x0001_0000_0001, 0, 0, 0, Bound::Exact, i16::MAX);
    let e = tt.probe(0x0001_0000_0001).unwrap();
    assert_eq!(e.raw_eval, i16::MAX);

    tt.store(0x0002_0000_0002, 0, 0, 0, Bound::Exact, i16::MIN);
    let e = tt.probe(0x0002_0000_0002).unwrap();
    assert_eq!(e.raw_eval, i16::MIN);

    tt.store(0x0003_0000_0003, 0, 0, 0, Bound::Exact, 150);
    let e = tt.probe(0x0003_0000_0003).unwrap();
    assert_eq!(e.raw_eval, 150);

    tt.store(0x0004_0000_0004, 0, 0, 0, Bound::Exact, -320);
    let e = tt.probe(0x0004_0000_0004).unwrap();
    assert_eq!(e.raw_eval, -320);
}
