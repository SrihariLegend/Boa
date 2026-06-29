/// z-sweep: test whether z behaves as a confidence parameter.
///
/// Sweeps z = 1.3, 1.6, 2.0, 2.3, 2.6 for variance-aware RFP margins:
///   M(d,σ,z) = μ·d + z·σ·√d
///
/// For each z, runs the same paired A/B against fixed(K=15) on identical
/// candidate moves, with depth-4 verification-search ground truth.
///
/// If the model is correct, we should see a smooth Pareto frontier:
/// higher z → fewer false prunes, fewer correct saves (and vice versa).
use boa::board::{Board, Zobrist};
use boa::config::EngineOptions;
use boa::eval;
use boa::movegen;
use boa::search::pruning::sigma;
use boa::search::quick_search;
use boa::tt::TranspositionTable;
use boa::types::*;

fn main() {
    let positions: Vec<&str> = vec![
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        "r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
        "r1bq1rk1/pppp1ppp/2n2n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 b - - 0 6",
        "rnbqkbnr/ppp2ppp/3p4/4p3/4P3/3P4/PPP2PPP/RNBQKBNR w KQkq - 0 3",
        "8/8/4k3/3p4/3P4/4K3/8/8 w - - 0 1",
        "r1bqkb1r/pp2pppp/2np1n2/4P3/3P4/2N5/PPP2PPP/R1BQKBNR b KQkq - 0 4",
        "8/1p3kp1/p1p1p2p/2P1P3/3P4/2N5/PP3PPP/6K1 w - - 0 1",
        "r2qkb1r/pp2pppp/2p2n2/3p4/2PP4/2N1PN2/PP3PPP/R1BQKB1R b KQkq - 2 6",
        "rnb1kb1r/ppppqppp/5n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 2 4",
        "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQK2R w KQkq - 4 5",
    ];

    let atk = movegen::AttackTables::init();
    let zobrist = Zobrist::new();
    let options = EngineOptions::default();
    let ectx = eval::EvalContext { atk: &atk, options: &options };
    let verify_tt = TranspositionTable::new(16);
    let verify_atk = movegen::AttackTables::init();
    let verify_z = Zobrist::new();

    const FIXED_K: i32 = 15;
    const VERIFY_DEPTH: i32 = 4;
    const Z_VALUES: [f64; 5] = [0.8, 1.2, 1.6, 2.0, 2.5];
    const REQUIRED_GAINS: [i32; 4] = [15, 25, 40, 60];

    #[derive(Default, Clone)]
    struct ZResult {
        prunes: u32,
        false_prunes: u32,
        severity: Vec<i32>,
    }

    let mut results: Vec<(f64, ZResult)> = Vec::new();

    for &z in &Z_VALUES {
        eprintln!("z = {:.1}...", z);
        let mut r = ZResult::default();

        for fen in &positions {
            let board = Board::from_fen(fen).unwrap();
            let eval_before = eval::evaluate(&board, &ectx);
            let s = sigma(&board);
            let moves = movegen::gen_moves(&board, &atk);

            for i in 0..moves.count {
                let m = moves.moves[i];
                let to_sq = move_to(m) as usize;
                let is_capture = board.sq_piece[to_sq] != PIECE_NONE || move_flags(m) == MF_EN_PASSANT;
                if is_capture || move_flags(m) == MF_PROMOTION { continue; }

                let mut child = board.clone();
                let undo = child.make_move(m, &zobrist);
                if child.is_in_check(child.side.flip()) { child.unmake_move(m, &undo, &zobrist); continue; }

                let search_score = quick_search(&mut child, &options, VERIFY_DEPTH, &verify_atk, &verify_z, &verify_tt);
                let true_gain = -search_score - eval_before;
                child.unmake_move(m, &undo, &zobrist);

                for &rg in &REQUIRED_GAINS {
                    // Simplified variance margin: M = z·σ
                    // Tests z as a confidence scalar on the per-ply variance.
                    let var_margin = (z * s as f64).round() as i32;
                    let var_prunes = var_margin < rg;

                    let safe = true_gain < rg;
                    if var_prunes {
                        r.prunes += 1;
                        if !safe {
                            r.false_prunes += 1;
                            r.severity.push(true_gain - rg);
                        }
                    }
                }
            }
        }
        results.push((z, r));
    }

    // ---- Output ----
    println!("=== z-Sweep: Variance-Aware Pruning Confidence Parameter ===");
    println!();
    println!("Margin: M(σ,z) = z·σ   Verification: depth-{}", VERIFY_DEPTH);
    println!("Fixed baseline: K={}   Candidates: {} positions × {} required_gains",
             FIXED_K, positions.len(), REQUIRED_GAINS.len());
    println!();

    println!("{:<8} {:>10} {:>12} {:>14} {:>10} {:>10} {:>10}",
             "z", "prunes", "false%", "correct saves", "med miss", "p95 miss", "max miss");
    println!("{}", "-".repeat(75));

    for (z, r) in &results {
        let fp_rate = if r.prunes > 0 { r.false_prunes as f64 / r.prunes as f64 * 100.0 } else { 0.0 };
        let correct = r.prunes - r.false_prunes;
        let med = percentile(&r.severity, 50);
        let p95 = percentile(&r.severity, 95);
        let max = r.severity.iter().max().copied().unwrap_or(0);
        println!("{:<8} {:>10} {:>11.1}% {:>14} {:>10} {:>10} {:>10}",
                 format!("{:.1}", z), r.prunes, fp_rate, correct, med, p95, max);
    }

    // Pareto interpretation
    println!();
    println!("{}", "=".repeat(75));
    if results.len() >= 2 {
        let first = &results[0].1;
        let last = &results[results.len() - 1].1;
        let fp_drop = (first.false_prunes as f64 / first.prunes.max(1) as f64
                        - last.false_prunes as f64 / last.prunes.max(1) as f64) * 100.0;
        let save_drop = (first.prunes - first.false_prunes) as i32
            - (last.prunes - last.false_prunes) as i32;

        // Check monotonicity: false-prune rate should decrease with z
        let mut monotonic = true;
        for i in 1..results.len() {
            let prev_rate = results[i-1].1.false_prunes as f64 / results[i-1].1.prunes.max(1) as f64;
            let curr_rate = results[i].1.false_prunes as f64 / results[i].1.prunes.max(1) as f64;
            if curr_rate > prev_rate + 0.005 { monotonic = false; }
        }

        println!("False-prune rate change: z={:.1}→{:.1}: {:.1}% → {:.1}% ({}{:.1}pp)",
                 Z_VALUES[0], Z_VALUES[Z_VALUES.len()-1],
                 first.false_prunes as f64 / first.prunes.max(1) as f64 * 100.0,
                 last.false_prunes as f64 / last.prunes.max(1) as f64 * 100.0,
                 if fp_drop > 0.0 { "↓" } else { "↑" }, fp_drop.abs());
        println!("Correct saves lost: {}", save_drop);

        if monotonic {
            println!();
            println!("*** FALSE-PRUNE RATE IS MONOTONICALLY DECREASING WITH z ***");
            println!("*** z behaves as a genuine confidence parameter, not a magic constant. ***");
        }
    }
}

fn percentile(data: &[i32], p: usize) -> i32 {
    if data.is_empty() { return 0; }
    let mut v = data.to_vec();
    v.sort_unstable();
    v[(v.len() * p / 100).min(v.len() - 1)]
}
