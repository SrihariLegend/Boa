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

#[test]
fn depth_zero_does_not_block_deeper_cross_key() {
    // With multiplication-based indexing, hashes 0..N all map to bucket 0
    // when num_buckets > 1 (since (small_hash * num_buckets) >> 64 == 0).
    // Fill bucket 0 with 3 depth-0 entries, then store a depth>0 entry
    // at another hash — it must evict a depth-0 entry, not be dropped.
    let tt = TranspositionTable::new(1);
    tt.new_search();

    // Fill bucket 0 with 3 depth-0 entries (different keys: 0, 1, 2)
    tt.store(0, 10, 0xAAAA, 0, Bound::Lower, 20);
    tt.store(1, 20, 0xBBBB, 0, Bound::Lower, 30);
    tt.store(2, 30, 0xCCCC, 0, Bound::Lower, 40);
    assert!(tt.probe(0).is_some());
    assert!(tt.probe(1).is_some());
    assert!(tt.probe(2).is_some());

    // Store a depth-5 entry at hash 3 (also bucket 0, different key).
    // Must evict one of the depth-0 entries, not be dropped.
    tt.store(3, 100, 0xDDDD, 5, Bound::Exact, 50);
    let e = tt.probe(3).expect("depth-5 store should not be dropped");
    assert_eq!(e.depth, 5);
    assert_eq!(e.score, 100);

    // At least one of the depth-0 entries must have been evicted
    // (3 depth-0 + 1 depth-5 = 4 entries → one must go).
    let survivors = [0u64, 1, 2].iter().filter(|h| tt.probe(**h).is_some()).count();
    assert!(survivors <= 2, "at least one depth-0 entry should be evicted, but {} survived", survivors);
}
