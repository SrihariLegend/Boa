/// Paired A/B comparison: fixed vs variance-aware futility margins.
///
/// For every quiet-move pruning candidate in a test suite, evaluates both
/// margin formulas against the SAME ground truth.  Produces:
///
///   1. A 2×2 decision matrix (fixed × variance vs ground truth)
///   2. False-prune rate for each algorithm
///   3. Node-savings rate (correct prunes / total candidates)
///   4. Per-σ calibration breakdown
///
/// Fixed margin:   M_fixed(d) = K · d
/// Variance margin: ffp_margin(FfpInput{...}) — history + index + depth terms
///
/// Ground truth: immediate eval change after one ply (cheap proxy).
use boa::board::{Board, Zobrist};
use boa::config::EngineOptions;
use boa::eval;
use boa::movegen;
use boa::search::pruning::{ffp_margin, sigma};
use boa::search::FfpInput;
use boa::types::*;

use std::collections::BTreeMap;

fn main() {
    let positions: Vec<&str> = vec![
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        "r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
        "r1bq1rk1/pppp1ppp/2n2n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 b - - 0 6",
        "rnbqkbnr/ppp2ppp/3p4/4p3/4P3/3P4/PPP2PPP/RNBQKBNR w KQkq - 0 3",
        "8/8/4k3/3p4/3P4/4K3/8/8 w - - 0 1",
        "r1bqkb1r/pp2pppp/2np1n2/4P3/3P4/2N5/PPP2PPP/R1BQKBNR b KQkq - 0 4",
        "r3kb1r/pppqpppp/2n2n2/3p4/3PP1b1/2N2N2/PPP1BPPP/R1BQK2R w KQkq - 4 5",
        "8/1p3kp1/p1p1p2p/2P1P3/3P4/2N5/PP3PPP/6K1 w - - 0 1",
        "r1bqkb1r/1ppp1ppp/p1n2n2/4p3/B3P3/5N2/PPPP1PPP/RNBQ1RK1 b kq - 0 5",
        "r2qkb1r/pp2pppp/2p2n2/3p4/2PP4/2N1PN2/PP3PPP/R1BQKB1R b KQkq - 2 6",
        "8/8/8/4k3/3p4/8/4K3/8 b - - 0 1",
        "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQK2R w KQkq - 4 5",
    ];

    let atk = movegen::AttackTables::init();
    let zobrist = Zobrist::new();
    let options = EngineOptions::default();
    let ectx = eval::EvalContext { atk: &atk, options: &options };

    const FIXED_K: i32 = 15; // M_fixed = K·d (30-60cp at d=2-4, comparable to variance range)

    #[derive(Default)]
    struct Counts {
        candidates: u32,
        both_prune_correct: u32,
        both_prune_wrong: u32,
        fixed_only_prune_correct: u32,
        fixed_only_prune_wrong: u32,
        var_only_prune_correct: u32,
        var_only_prune_wrong: u32,
        both_search_wrong: u32,
        both_search_correct: u32,
    }

    let mut counts = Counts::default();
    let mut bucket_counts: BTreeMap<i32, (u32, u32)> = BTreeMap::new(); // σ→(pruned, false_prunes)

    for fen in &positions {
        let board = Board::from_fen(fen).unwrap();
        let eval_before = eval::evaluate(&board, &ectx);
        let s = sigma(&board);
        let moves = movegen::gen_moves(&board, &atk);

        for i in 0..moves.count {
            let m = moves.moves[i];
            let to_sq = move_to(m) as usize;
            let is_capture = board.sq_piece[to_sq] != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
            let is_promo = move_flags(m) == MF_PROMOTION;
            if is_capture || is_promo { continue; }

            let mut b2 = board.clone();
            let undo = b2.make_move(m, &zobrist);
            if b2.is_in_check(b2.side.flip()) {
                b2.unmake_move(m, &undo, &zobrist);
                continue;
            }

            let eval_after = eval::evaluate(&b2, &ectx);
            let true_gain = -eval_after - eval_before;
            b2.unmake_move(m, &undo, &zobrist);

            // Test across FFP-relevant depths and gain thresholds
            for depth in 2..=4 {
                for &required_gain in &[20, 30, 50] {
                    let fixed_margin = FIXED_K * depth;
                    let fixed_prunes = fixed_margin < required_gain;

                    let ffp_input = FfpInput {
                        depth,
                        static_eval: eval_before,
                        alpha: eval_before + required_gain,
                        move_index: 10,
                        is_cut_node: false,
                        history_score: 0,
                        sigma: s,
                    };
                    let var_margin = ffp_margin(ffp_input);
                    let var_prunes = var_margin < required_gain;

                    let safe_to_prune = true_gain < required_gain;
                    counts.candidates += 1;

                    match (fixed_prunes, var_prunes, safe_to_prune) {
                        (true, true, true) => counts.both_prune_correct += 1,
                        (true, true, false) => counts.both_prune_wrong += 1,
                        (true, false, true) => counts.fixed_only_prune_correct += 1,
                        (true, false, false) => counts.fixed_only_prune_wrong += 1,
                        (false, true, true) => counts.var_only_prune_correct += 1,
                        (false, true, false) => counts.var_only_prune_wrong += 1,
                        (false, false, true) => counts.both_search_wrong += 1,
                        (false, false, false) => counts.both_search_correct += 1,
                    }

                    if var_prunes {
                        let bk = (s / 4) * 4;
                        let e = bucket_counts.entry(bk).or_default();
                        e.0 += 1;
                        if !safe_to_prune { e.1 += 1; }
                    }
                }
            }
        }
    }

    // ---- 2×2 Decision Matrix ----
    println!("=== Paired A/B: Fixed(K={}) vs Variance-Aware Futility Margins ===", FIXED_K);
    println!();
    println!("Ground truth: 1-ply static eval change    N = {}", counts.candidates);
    println!();

    let fixed_prunes = counts.both_prune_correct + counts.both_prune_wrong
        + counts.fixed_only_prune_correct + counts.fixed_only_prune_wrong;
    let var_prunes = counts.both_prune_correct + counts.both_prune_wrong
        + counts.var_only_prune_correct + counts.var_only_prune_wrong;
    let fixed_fp = counts.both_prune_wrong + counts.fixed_only_prune_wrong;
    let var_fp = counts.both_prune_wrong + counts.var_only_prune_wrong;
    let fixed_correct = fixed_prunes.saturating_sub(fixed_fp);
    let var_correct = var_prunes.saturating_sub(var_fp);

    println!("2x2 DECISION MATRIX (Fixed rows, Variance cols)");
    println!("Each cell: (correct prunes, WRONG prunes)");
    println!();
    println!("                       Variance PRUNES   Variance SEARCHES");
    println!("Fixed PRUNES           ({:>5}, {:>5})    ({:>5}, {:>5})",
             counts.both_prune_correct, counts.both_prune_wrong,
             counts.fixed_only_prune_correct, counts.fixed_only_prune_wrong);
    println!("Fixed SEARCHES         ({:>5}, {:>5})    ({:>5}, {:>5})",
             counts.var_only_prune_correct, counts.var_only_prune_wrong,
             counts.both_search_correct, counts.both_search_wrong);

    println!();
    println!("{}", "=".repeat(70));
    println!("PER-ALGORITHM METRICS");
    println!("{:<25} {:>12} {:>12} {:>12}", "Metric", "Fixed", "Variance", "Delta");
    println!("{}", "-".repeat(60));

    let fixed_fp_rate = if fixed_prunes > 0 { fixed_fp as f64 / fixed_prunes as f64 * 100.0 } else { 0.0 };
    let var_fp_rate = if var_prunes > 0 { var_fp as f64 / var_prunes as f64 * 100.0 } else { 0.0 };
    let fixed_prune_rate = fixed_prunes as f64 / counts.candidates as f64 * 100.0;
    let var_prune_rate = var_prunes as f64 / counts.candidates as f64 * 100.0;
    let fixed_precision = if fixed_prunes > 0 { fixed_correct as f64 / fixed_prunes as f64 * 100.0 } else { 100.0 };
    let var_precision = if var_prunes > 0 { var_correct as f64 / var_prunes as f64 * 100.0 } else { 100.0 };

    println!("{:<25} {:>11}% {:>11}% {:>+11}%", "Prune rate",
             format!("{:.1}", fixed_prune_rate), format!("{:.1}", var_prune_rate),
             format!("{:.1}", var_prune_rate - fixed_prune_rate));
    println!("{:<25} {:>11}% {:>11}% {:>+11}%", "False-prune rate",
             format!("{:.1}", fixed_fp_rate), format!("{:.1}", var_fp_rate),
             format!("{:.1}", var_fp_rate - fixed_fp_rate));
    println!("{:<25} {:>11}% {:>11}% {:>+11}%", "Precision (correct/all)",
             format!("{:.1}", fixed_precision), format!("{:.1}", var_precision),
             format!("{:.1}", var_precision - fixed_precision));
    let fixed_saved = counts.both_prune_correct + counts.fixed_only_prune_correct;
    let var_saved = counts.both_prune_correct + counts.var_only_prune_correct;
    let node_saving_delta = var_saved as i32 - fixed_saved as i32;
    println!("{:<25} {:>11} {:>11} {:>+11}", "Correct prunes (nodes saved)",
             fixed_saved, var_saved, node_saving_delta);

    println!();
    println!("{}", "=".repeat(70));
    println!("DISAGREEMENTS (where algorithms differ)");
    println!();
    let fixed_only_total = counts.fixed_only_prune_correct + counts.fixed_only_prune_wrong;
    let var_only_total = counts.var_only_prune_correct + counts.var_only_prune_wrong;
    println!("Fixed prunes, Variance searches: {} decisions ({} correct, {} WRONG)",
             fixed_only_total, counts.fixed_only_prune_correct, counts.fixed_only_prune_wrong);
    println!("Variance prunes, Fixed searches: {} decisions ({} correct, {} WRONG)",
             var_only_total, counts.var_only_prune_correct, counts.var_only_prune_wrong);

    if counts.var_only_prune_wrong == 0 && var_only_total > 0 {
        println!();
        println!("*** All variance-only prunes are correct — variance is strictly more precise ***");
    }
    if counts.fixed_only_prune_wrong > counts.var_only_prune_wrong && fixed_only_total > 0 {
        println!();
        println!("*** Fixed margin makes {} more wrong prunes than variance ***",
                 counts.fixed_only_prune_wrong - counts.var_only_prune_wrong);
    }

    // Per-σ calibration
    println!();
    println!("{}", "=".repeat(70));
    println!("CALIBRATION: False-prune rate vs σ (variance margin only)");
    println!("{:<10} {:>8} {:>12} {:>12}", "σ bucket", "prunes", "false", "rate");
    println!("{}", "-".repeat(45));
    for (bk, (total, fp)) in &bucket_counts {
        let rate = if *total > 0 { *fp as f64 / *total as f64 * 100.0 } else { 0.0 };
        let bar = "█".repeat((rate * 2.0) as usize);
        println!("σ≈{:<7} {:>8} {:>8}    {:>5.1}%  {}", bk, total, fp, rate, bar);
    }

    println!();
    println!("Caveats:");
    println!("  - Ground truth = 1-ply static eval (not depth-N search)");
    println!("  - Single move_index=10, history=0 for all candidates");
    println!("  - For rigorous results: sweep these params + use verification search");
}
