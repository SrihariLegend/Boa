use super::*;

#[test]
pub(super) fn uci_option_names_accept_spaces_and_case() {
    let mut options = EngineOptions::default();

    assert!(options.set_uci_option("Threads", "4"));
    assert_eq!(options.search.threads, 4);

    assert!(options.set_uci_option("Search Lazy SMP", "false"));
    assert!(!options.search.lazy_smp);

    assert!(options.set_uci_option("Search SEE QSearch Pruning", "false"));
    assert!(!options.search.see_qsearch_pruning);

    assert!(options.set_uci_option("Search Forward Futility Pruning", "false"));
    assert!(!options.search.forward_futility_pruning);

    assert!(options.set_uci_option("Syzygy Path", "/tmp/tb"));
    assert_eq!(options.syzygy.path, "/tmp/tb");
}

#[test]
pub(super) fn eval_scales_are_clamped() {
    let mut options = EngineOptions::default();

    assert!(options.set_uci_option("Eval Mobility Scale", "999"));
    assert_eq!(options.eval.mobility_scale, 300);

    assert!(options.set_uci_option("Eval Mobility Scale", "-10"));
    assert_eq!(options.eval.mobility_scale, 0);

    assert!(options.set_uci_option("Syzygy Probe Limit", "99"));
    assert_eq!(options.syzygy.probe_limit, 6);
}
