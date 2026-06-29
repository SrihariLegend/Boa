/// Verification-search ground truth for pruning A/B comparison.
///
/// For each quiet-move pruning candidate:
///   1. Records σ(pos), features, both margin decisions
///   2. Runs a depth-N search from the child position for TRUE ground truth
///   3. Measures false-prune severity (not just rate)
///   4. Produces: 2×2 matrix, severity distribution, σ calibration
///
/// This replaces the 1-ply static-eval proxy with proper alpha-beta search,
/// addressing the dominant uncertainty in the earlier A/B results.
use boa::board::{Board, Zobrist};
use boa::config::EngineOptions;
use boa::eval;
use boa::movegen;
use boa::search::pruning::{ffp_margin, sigma};
use boa::search::{quick_search, FfpInput};
use boa::types::*;

use std::collections::BTreeMap;

fn main() {
    // Representative positions — opening, middlegame, endgame, open/closed
    let positions: Vec<&str> = vec![
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        "r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
        "r1bq1rk1/pppp1ppp/2n2n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 b - - 0 6",
        "rnbqkbnr/ppp2ppp/3p4/4p3/4P3/3P4/PPP2PPP/RNBQKBNR w KQkq - 0 3",
        "8/8/4k3/3p4/3P4/4K3/8/8 w - - 0 1",
        "r1bqkb1r/pp2pppp/2np1n2/4P3/3P4/2N5/PPP2PPP/R1BQKBNR b KQkq - 0 4",
        "8/1p3kp1/p1p1p2p/2P1P3/3P4/2N5/PP3PPP/6K1 w - - 0 1",
        "r2qkb1r/pp2pppp/2p2n2/3p4/2PP4/2N1PN2/PP3PPP/R1BQKB1R b KQkq - 2 6",
    ];

    let atk = movegen::AttackTables::init();
    let zobrist = Zobrist::new();
    let options = EngineOptions::default();
    let ectx = eval::EvalContext { atk: &atk, options: &options };

    // Reusable structures for verification searches (avoid per-call allocation)
    let verify_tt = boa::tt::TranspositionTable::new(16);
    let verify_atk = movegen::AttackTables::init();
    let verify_z = Zobrist::new();

    const FIXED_K: i32 = 15;
    const VERIFY_DEPTH: i32 = 4; // depth for verification search from child

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
    let mut severity: Vec<i32> = Vec::new(); // missed scores for false prunes
    let mut bucket_counts: BTreeMap<i32, (u32, u32)> = BTreeMap::new();

    let total_positions = positions.len();
    let mut done = 0;

    for fen in &positions {
        done += 1;
        let board = Board::from_fen(fen).unwrap();
        let eval_before = eval::evaluate(&board, &ectx);
        let s = sigma(&board);
        let moves = movegen::gen_moves(&board, &atk);

        eprintln!("[{}/{}] {} quiet moves, σ={}", done, total_positions, moves.count, s);

        for i in 0..moves.count {
            let m = moves.moves[i];
            let to_sq = move_to(m) as usize;
            let is_capture = board.sq_piece[to_sq] != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
            let is_promo = move_flags(m) == MF_PROMOTION;
            if is_capture || is_promo { continue; }

            // Make move and verify legality
            let mut child_board = board.clone();
            let undo = child_board.make_move(m, &zobrist);
            if child_board.is_in_check(child_board.side.flip()) {
                child_board.unmake_move(m, &undo, &zobrist);
                continue;
            }

            // ---- Ground truth: depth-N search from child position ----
            // quick_search returns score from STM perspective (opponent's).
            // From our perspective: true_value = -search_score
            let search_score = quick_search(&mut child_board, &options, VERIFY_DEPTH,
                                              &verify_atk, &verify_z, &verify_tt);
            let true_gain = -search_score - eval_before;

            child_board.unmake_move(m, &undo, &zobrist);

            // ---- Pruning decisions ----
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
                        (true, true, false) => {
                            counts.both_prune_wrong += 1;
                            severity.push(true_gain - required_gain); // how much we missed by
                        }
                        (true, false, true) => counts.fixed_only_prune_correct += 1,
                        (true, false, false) => {
                            counts.fixed_only_prune_wrong += 1;
                            severity.push(true_gain - required_gain);
                        }
                        (false, true, true) => counts.var_only_prune_correct += 1,
                        (false, true, false) => {
                            counts.var_only_prune_wrong += 1;
                            severity.push(true_gain - required_gain);
                        }
                        (false, false, true) => counts.both_search_wrong += 1,
                        (false, false, false) => counts.both_search_correct += 1,
                    }

                    if var_prunes && !safe_to_prune {
                        let bk = (s / 4) * 4;
                        let e = bucket_counts.entry(bk).or_default();
                        e.0 += 1;
                        e.1 += 1;
                    } else if var_prunes {
                        let bk = (s / 4) * 4;
                        let e = bucket_counts.entry(bk).or_default();
                        e.0 += 1;
                    }
                }
            }
        }
    }

    // ---- Output ----
    println!("=== Verification-Search A/B: Fixed(K={}) vs Variance ===", FIXED_K);
    println!();
    println!("Verification depth: {}   Candidates: {}   Positions: {}",
             VERIFY_DEPTH, counts.candidates, total_positions);
    println!();

    // 2×2 matrix
    println!("2x2 DECISION MATRIX (correct prunes, WRONG prunes)");
    println!();
    println!("                       Variance PRUNES   Variance SEARCHES");
    println!("Fixed PRUNES           ({:>5}, {:>5})    ({:>5}, {:>5})",
             counts.both_prune_correct, counts.both_prune_wrong,
             counts.fixed_only_prune_correct, counts.fixed_only_prune_wrong);
    println!("Fixed SEARCHES         ({:>5}, {:>5})    ({:>5}, {:>5})",
             counts.var_only_prune_correct, counts.var_only_prune_wrong,
             counts.both_search_correct, counts.both_search_wrong);

    // Per-algorithm metrics
    let fixed_prunes = counts.both_prune_correct + counts.both_prune_wrong
        + counts.fixed_only_prune_correct + counts.fixed_only_prune_wrong;
    let var_prunes = counts.both_prune_correct + counts.both_prune_wrong
        + counts.var_only_prune_correct + counts.var_only_prune_wrong;
    let fixed_fp = counts.both_prune_wrong + counts.fixed_only_prune_wrong;
    let var_fp = counts.both_prune_wrong + counts.var_only_prune_wrong;
    let fixed_correct = fixed_prunes.saturating_sub(fixed_fp);
    let var_correct = var_prunes.saturating_sub(var_fp);

    println!();
    println!("{}", "=".repeat(70));
    println!("PER-ALGORITHM METRICS (ground truth = depth-{} search)", VERIFY_DEPTH);
    println!("{:<30} {:>12} {:>12} {:>12}", "Metric", "Fixed", "Variance", "Delta");
    println!("{}", "-".repeat(65));

    let fixed_pr = fixed_prunes as f64 / counts.candidates as f64 * 100.0;
    let var_pr = var_prunes as f64 / counts.candidates as f64 * 100.0;
    println!("{:<30} {:>11.1}% {:>11.1}% {:>+11.1}%", "Prune rate", fixed_pr, var_pr, var_pr - fixed_pr);

    let fixed_fpr = if fixed_prunes > 0 { fixed_fp as f64 / fixed_prunes as f64 * 100.0 } else { 0.0 };
    let var_fpr = if var_prunes > 0 { var_fp as f64 / var_prunes as f64 * 100.0 } else { 0.0 };
    println!("{:<30} {:>11.1}% {:>11.1}% {:>+11.1}%", "False-prune rate", fixed_fpr, var_fpr, var_fpr - fixed_fpr);

    let fixed_prec = if fixed_prunes > 0 { fixed_correct as f64 / fixed_prunes as f64 * 100.0 } else { 100.0 };
    let var_prec = if var_prunes > 0 { var_correct as f64 / var_prunes as f64 * 100.0 } else { 100.0 };
    println!("{:<30} {:>11.1}% {:>11.1}% {:>+11.1}%", "Precision", fixed_prec, var_prec, var_prec - fixed_prec);

    let fixed_saved = counts.both_prune_correct + counts.fixed_only_prune_correct;
    let var_saved = counts.both_prune_correct + counts.var_only_prune_correct;
    println!("{:<30} {:>11}  {:>11}  {:>+11}", "Correct prunes (saved)", fixed_saved, var_saved,
             var_saved as i32 - fixed_saved as i32);

    // Ratio
    let additional_correct = var_saved.saturating_sub(fixed_saved);
    let additional_wrong = var_fp.saturating_sub(fixed_fp);
    if additional_wrong > 0 {
        println!("{:<30} {:>11.1}x", "Correct/wrong ratio",
                 additional_correct as f64 / additional_wrong as f64);
    }

    // Disagreements
    println!();
    println!("{}", "=".repeat(70));
    println!("DISAGREEMENTS");
    let fixed_only = counts.fixed_only_prune_correct + counts.fixed_only_prune_wrong;
    let var_only = counts.var_only_prune_correct + counts.var_only_prune_wrong;
    println!("Fixed prunes  Variance searches: {} ({} correct, {} WRONG)",
             fixed_only, counts.fixed_only_prune_correct, counts.fixed_only_prune_wrong);
    println!("Variance prunes  Fixed searches: {} ({} correct, {} WRONG)",
             var_only, counts.var_only_prune_correct, counts.var_only_prune_wrong);

    // ---- Severity distribution ----
    if !severity.is_empty() {
        severity.sort_unstable();
        let n = severity.len();
        println!();
        println!("{}", "=".repeat(70));
        println!("FALSE-PRUNE SEVERITY DISTRIBUTION (missed score, n={})", n);
        println!();
        println!("  Min:     {:>6} cp", severity[0]);
        println!("  P25:     {:>6} cp", severity[n / 4]);
        println!("  Median:  {:>6} cp", severity[n / 2]);
        println!("  P75:     {:>6} cp", severity[3 * n / 4]);
        println!("  P95:     {:>6} cp", severity[(n * 95 / 100).min(n - 1)]);
        println!("  Max:     {:>6} cp", severity[n - 1]);

        // Bucket severity
        let mut sev_bins = BTreeMap::new();
        for &s in &severity {
            let bin = (s / 20) * 20;
            *sev_bins.entry(bin).or_insert(0u32) += 1;
        }
        println!();
        println!("  Severity histogram (20cp bins):");
        for (bin, count) in &sev_bins {
            let bar = "█".repeat(*count as usize);
            println!("  {:>4}-{:>4} cp: {:>3}  {}", bin, bin + 20, count, bar);
        }
    }

    // σ calibration
    println!();
    println!("{}", "=".repeat(70));
    println!("CALIBRATION: False-prune rate vs σ (variance margin, depth-{} verification)", VERIFY_DEPTH);
    println!("{:<10} {:>8} {:>8} {:>12}", "σ bucket", "prunes", "wrong", "rate");
    println!("{}", "-".repeat(40));
    for (bk, (total, fp)) in &bucket_counts {
        let rate = if *total > 0 { *fp as f64 / *total as f64 * 100.0 } else { 0.0 };
        let bar = "█".repeat((rate * 2.0) as usize);
        println!("σ≈{:<7} {:>8} {:>8} {:>8.1}%  {}", bk, total, fp, rate, bar);
    }

    println!();
    println!("Ground truth: depth-{} alpha-beta search from the child position.", VERIFY_DEPTH);
    println!("This replaces the 1-ply static-eval proxy used in earlier diagnostics.");
}
