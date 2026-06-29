/// Pruning calibration: measures false-prune rate vs σ(pos) to validate the
/// variance-aware model.
///
/// For each quiet move in a set of test positions:
///   1. Records σ(pos), depth, static_eval
///   2. Computes the variance-aware margin and a fixed baseline margin
///   3. Simulates FFP pruning: always prune, then check if true_gain >= required_gain
///   4. Buckets by σ and computes false-prune rates
///
/// Hypothesis: if σ genuinely predicts decision risk, false-prune rates should
/// be higher in high-σ positions. If they are flat, the model is wrong.
///
/// Limitation: uses immediate static eval change as ground truth proxy.
/// For rigorous results, substitute a depth-N verification search.
use boa::board::{Board, Zobrist};
use boa::config::EngineOptions;
use boa::eval;
use boa::movegen;
use boa::search::pruning::{rfp_margin, sigma};
use boa::types::*;

use std::collections::BTreeMap;

fn main() {
    let positions: Vec<(&str, &str)> = vec![
        ("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1", "start"),
        ("r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4", "open"),
        ("r1bq1rk1/pppp1ppp/2n2n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 b - - 0 6", "mid"),
        ("rnbqkbnr/ppp2ppp/3p4/4p3/4P3/3P4/PPP2PPP/RNBQKBNR w KQkq - 0 3", "closed"),
        ("8/8/4k3/3p4/3P4/4K3/8/8 w - - 0 1", "endgame"),
        ("r1bqkb1r/pp2pppp/2np1n2/4P3/3P4/2N5/PPP2PPP/R1BQKBNR b KQkq - 0 4", "sicilian"),
        ("r3kb1r/pppqpppp/2n2n2/3p4/3PP1b1/2N2N2/PPP1BPPP/R1BQK2R w KQkq - 4 5", "sharp"),
        ("8/1p3kp1/p1p1p2p/2P1P3/3P4/2N5/PP3PPP/6K1 w - - 0 1", "locked"),
    ];

    let atk = movegen::AttackTables::init();
    let zobrist = Zobrist::new();
    let options = EngineOptions::default();
    let ectx = eval::EvalContext { atk: &atk, options: &options };

    const FIXED_K: i32 = 50; // reference fixed margin: M(d) = K * d

    #[derive(Default)]
    struct Bucket {
        count: u32,
        false_prunes: u32,
        total_margin_var: i64,
        total_margin_fixed: i64,
    }

    let mut buckets: BTreeMap<i32, Bucket> = BTreeMap::new();

    for (fen, _label) in &positions {
        let board = Board::from_fen(fen).unwrap();
        let eval_before = eval::evaluate(&board, &ectx);
        let s = sigma(&board);
        let moves = movegen::gen_moves(&board, &atk);

        for i in 0..moves.count {
            let m = moves.moves[i];
            let to_sq = move_to(m) as usize;
            let is_capture = board.sq_piece[to_sq] != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
            let is_promo = move_flags(m) == MF_PROMOTION;
            if is_capture || is_promo {
                continue;
            }

            let mut b2 = board.clone();
            let undo = b2.make_move(m, &zobrist);
            if b2.is_in_check(b2.side.flip()) {
                b2.unmake_move(m, &undo, &zobrist);
                continue;
            }

            // Immediate eval swing from our perspective: -eval_after - eval_before
            let eval_after = eval::evaluate(&b2, &ectx);
            let true_gain = -eval_after - eval_before;

            // Simulate FFP: always prune this quiet move.
            // Ground truth: the move should have been searched if true_gain >= required_gain.
            let required_gain = 30; // cp — typical α − static_eval gap
            let false_prune = true_gain >= required_gain;

            // Margins at depth=3 for reporting
            let depth = 3;
            let var_margin = rfp_margin(depth, s);
            let fixed_margin = FIXED_K * depth;

            let bucket_key = (s / 4) * 4;
            let b = buckets.entry(bucket_key).or_default();
            b.count += 1;
            if false_prune {
                b.false_prunes += 1;
            }
            b.total_margin_var += var_margin as i64;
            b.total_margin_fixed += fixed_margin as i64;

            b2.unmake_move(m, &undo, &zobrist);
        }
    }

    println!("=== False-Prune Rate vs σ(pos) ===");
    println!();
    println!("Simulates FFP prune on ALL quiet moves (required_gain = 30 cp)");
    println!("False prune = pruned but true_gain >= required_gain");
    println!();

    let fixed_label = format!("fixed(K={})", FIXED_K);
    println!("{:<10} {:>8} {:>12} {:>14} {:>14}",
             "σ bucket", "N", "false%", "var_margin", fixed_label);
    println!("{}", "-".repeat(60));

    for (key, b) in &buckets {
        let fp_rate = if b.count > 0 {
            b.false_prunes as f64 / b.count as f64 * 100.0
        } else {
            0.0
        };
        let avg_var = if b.count > 0 { b.total_margin_var as f64 / b.count as f64 } else { 0.0 };
        let avg_fixed = if b.count > 0 { b.total_margin_fixed as f64 / b.count as f64 } else { 0.0 };
        println!("σ≈{:<7} {:>8} {:>11.1}% {:>14.1} {:>14.1}",
                 key, b.count, fp_rate, avg_var, avg_fixed);
    }

    // Summary
    if buckets.len() >= 2 {
        let keys: Vec<i32> = buckets.keys().copied().collect();
        let lo_key = keys[0];
        let hi_key = keys[keys.len() - 1];
        let lo = &buckets[&lo_key];
        let hi = &buckets[&hi_key];
        let lo_rate = lo.false_prunes as f64 / lo.count.max(1) as f64 * 100.0;
        let hi_rate = hi.false_prunes as f64 / hi.count.max(1) as f64 * 100.0;

        println!();
        println!("{}", "=".repeat(60));
        println!("Low-σ  (σ≈{}): false prune rate = {:.1}% (n={})", lo_key, lo_rate, lo.count);
        println!("High-σ (σ≈{}): false prune rate = {:.1}% (n={})", hi_key, hi_rate, hi.count);

        if hi_rate > 1.4 * lo_rate && lo_rate > 0.0 {
            println!();
            println!("*** σ PREDICTS DECISION RISK: {:.1}x higher false-prune rate in high-σ ***",
                     hi_rate / lo_rate);
            println!("*** Variance-aware margins that widen with σ are justified. ***");
        } else if (hi_rate - lo_rate).abs() < 2.0 {
            println!();
            println!("*** Flat: false-prune rates are similar across σ buckets. ***");
            println!("*** Either σ doesn't predict risk or fixed margin is well-tuned. ***");
        } else {
            println!();
            println!("*** Moderate signal: σ shows some predictive power for decision risk. ***");
            println!("*** Calibration data supports further investigation (SPRT). ***");
        }
    }

    println!();
    println!("Caveat: ground truth = immediate static eval after one ply, not search.");
    println!("For rigorous results use `criticality` probe data with full re-search.");
}
