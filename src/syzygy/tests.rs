use super::*;

#[test]
pub(super) fn empty_path_disables_tablebases() {
    assert!(SyzygyTablebase::load("").unwrap().is_none());
    assert!(SyzygyTablebase::load("<empty>").unwrap().is_none());
}

#[test]
pub(super) fn probe_limit_caps_at_six_pieces() {
    let mut options = SyzygyOptions::default();
    options.probe_limit = 6;
    let board = Board::from_fen("8/8/8/8/8/8/4K3/4k3 w - - 0 1").unwrap();
    assert!(can_probe(&board, &options, 6, 1));
}
