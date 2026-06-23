use super::*;
pub(super) fn print_engine_options() {
    let defaults = EngineOptions::default();
    for (name, default) in [
        ("Eval Material Scale", defaults.eval.material_scale),
        ("Eval PST Scale", defaults.eval.pst_scale),
        ("Eval Mobility Scale", defaults.eval.mobility_scale),
        (
            "Eval Pawn Structure Scale",
            defaults.eval.pawn_structure_scale,
        ),
        ("Eval King Safety Scale", defaults.eval.king_safety_scale),
    ] {
        println!(
            "option name {} type spin default {} min 0 max 300",
            name, default
        );
    }
    for name in [
        "Search Lazy SMP",
        "Search SEE",
        "Search SEE QSearch Pruning",
        "Search SEE Capture Ordering",
        "Search Forward Futility Pruning",
    ] {
        println!("option name {} type check default true", name);
    }
}
