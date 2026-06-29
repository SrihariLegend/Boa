/// Diagnostic: measure eval-change variance across DIVERSE position types.
///
/// Hypothesis: positions with high complexity (mobility, open files, king danger)
/// have larger eval-swing variance after quiet moves. Fixed futility margins
/// cannot be optimal across both calm and volatile positions.
use boa::board::{Board, Zobrist};
use boa::config::EngineOptions;
use boa::eval;
use boa::movegen;
use boa::types::{move_flags, move_to, MF_EN_PASSANT, MF_PROMOTION, PIECE_NONE};

fn main() {
    let positions: Vec<(&str, &str)> = vec![
        // ---- LOCKED / CALM (low variance expected) ----
        ("r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 b kq - 2 6", "calm-mid"),
        ("r1bqkb1r/pppp1ppp/2n2n2/4p3/2P5/2N2NP1/PP1PPPBP/R1BQK2R b KQkq - 1 5", "calm-english"),
        ("8/1p3kp1/p1p1p2p/2P1P3/3P4/2N5/PP3PPP/6K1 w - - 0 1", "locked-endgame"),
        ("8/p3k3/1p2p2p/3pPp2/3P1P2/2P3P1/PP5P/6K1 w - - 0 1", "full-lock"),
        // ---- OPEN / TACTICAL (high variance expected) ----
        ("r1bqkb1r/pp2pppp/2np1n2/4P3/3P4/2N5/PPP2PPP/R1BQKBNR b KQkq - 0 4", "sicilian-open"),
        ("r1bq1rk1/pppp1ppp/5n2/2b1p3/2BnP3/2NP1N2/PPP2PPP/R1BQ1RK1 b - - 2 6", "tactical-pin"),
        ("rnb1kb1r/ppppqppp/5n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 2 4", "kings-gambit-ish"),
        ("r3kb1r/pppqpppp/2n2n2/3p4/3PP1b1/2N2N2/PPP1BPPP/R1BQK2R w KQkq - 4 5", "sharp-center"),
        // ---- ENDGAME (very different phase) ----
        ("8/8/4k3/3p4/3P4/4K3/8/8 w - - 0 1", "kp-endgame"),
        ("8/5p2/4k3/3p4/3P4/4K3/8/8 w - - 0 1", "kpp-endgame"),
        // ---- WIDE OPEN (extreme) ----
        ("r1bqk2r/pppp1ppp/2n5/4p3/2B1P1n1/2NP1N2/PPP2PPP/R1BQ1RK1 w kq - 0 6", "hanging-pieces"),
        ("r1bq1rk1/ppp2ppp/2np1n2/4p3/2PPP3/2N2N2/PP3PPP/R1BQKB1R w KQ - 0 6", "open-center"),
    ];

    let atk = movegen::AttackTables::init();
    let zobrist = Zobrist::new();
    let options = EngineOptions::default();
    let ctx = eval::EvalContext {
        atk: &atk,
        options: &options,
    };

    println!("=== Eval-Swing Variance Diagnostic ===\n");
    println!("Measuring |eval_change| after each legal QUIET move.\n");

    let header = format!(
        "{:<22} {:>3} {:>3} {:>8} {:>8} {:>8} {:>8}",
        "Position", "Tot", "Q", "Mean|d|", "Std|d|", "P95|d|", "Max|d|"
    );
    println!("{}", header);
    println!("{}", "-".repeat(header.len()));

    // Collect stats per position for correlation
    let mut rows = Vec::new();

    for (fen, label) in &positions {
        let board = Board::from_fen(fen).unwrap();
        let eval_before = eval::evaluate(&board, &ctx);
        let moves = movegen::gen_moves(&board, &atk);
        let mobility = moves.count;

        let mut deltas = Vec::new();
        for i in 0..mobility {
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
            let eval_after = eval::evaluate(&b2, &ctx);
            // After our move, STM is the opponent. eval_after is from opponent's perspective.
            // From our perspective the eval is -eval_after.
            // Swing = (-eval_after) - eval_before
            let swing = -eval_after - eval_before;
            deltas.push(swing.abs());
            b2.unmake_move(m, &undo, &zobrist);
        }

        let quiet_count = deltas.len();
        if quiet_count == 0 {
            continue;
        }

        let mean = deltas.iter().sum::<i32>() as f64 / quiet_count as f64;
        let var = deltas.iter()
            .map(|d| (*d as f64 - mean).powi(2))
            .sum::<f64>() / quiet_count as f64;
        let std = var.sqrt();
        let mut sorted = deltas.clone();
        sorted.sort_unstable();
        let p95 = sorted[(quiet_count * 95 / 100).min(quiet_count - 1)];
        let max_delta = sorted[quiet_count - 1];

        println!(
            "{:<22} {:>3} {:>3} {:>8.1} {:>8.1} {:>8} {:>8}",
            label, mobility, quiet_count, mean, std, p95, max_delta
        );

        rows.push((label, mobility, quiet_count, mean, std, p95, max_delta));
    }

    // Group: calm vs volatile based on mobility + max delta
    println!();
    println!("{}", "=".repeat(70));
    println!("GROUP ANALYSIS: splitting by quiet-move count (proxy for position complexity)");
    println!();

    let med = 25; // median-ish quiet moves

    let mut lo_group = Vec::new();
    let mut hi_group = Vec::new();
    for &(label, _, q, mean, std, p95, max_d) in &rows {
        if q <= med {
            lo_group.push((label, q, mean, std, p95, max_d));
        } else {
            hi_group.push((label, q, mean, std, p95, max_d));
        }
    }

    for (name, group) in &[("Low-complexity (<=25 quiet moves)", &lo_group),
                            ("High-complexity (>25 quiet moves)", &hi_group)] {
        if group.is_empty() { continue; }
        let n: usize = group.iter().map(|(_, q, _, _, _, _)| *q).sum();
        let avg_mean = group.iter().map(|(_, _, m, _, _, _)| m).sum::<f64>() / group.len() as f64;
        let avg_std = group.iter().map(|(_, _, _, s, _, _)| s).sum::<f64>() / group.len() as f64;
        let avg_p95 = group.iter().map(|(_, _, _, _, p, _)| *p as f64).sum::<f64>() / group.len() as f64;
        let avg_max = group.iter().map(|(_, _, _, _, _, m)| *m as f64).sum::<f64>() / group.len() as f64;

        println!("{} ({} positions, {} moves):", name, group.len(), n);
        println!("  avg mean|d| = {:.1}   avg std|d| = {:.1}   avg P95 = {:.1}   avg max = {:.1}",
                 avg_mean, avg_std, avg_p95, avg_max);
    }

    // Key comparison
    if !lo_group.is_empty() && !hi_group.is_empty() {
        let lo_avg_std = lo_group.iter().map(|(_, _, _, s, _, _)| s).sum::<f64>() / lo_group.len() as f64;
        let hi_avg_std = hi_group.iter().map(|(_, _, _, s, _, _)| s).sum::<f64>() / hi_group.len() as f64;
        let lo_avg_max = lo_group.iter().map(|(_, _, _, _, _, m)| *m as f64).sum::<f64>() / lo_group.len() as f64;
        let hi_avg_max = hi_group.iter().map(|(_, _, _, _, _, m)| *m as f64).sum::<f64>() / hi_group.len() as f64;

        println!();
        println!("{}", "-".repeat(70));
        if lo_avg_std > 0.0 {
            println!("Std ratio (hi/lo): {:.2}x", hi_avg_std / lo_avg_std);
        }
        if lo_avg_max > 0.0 {
            println!("Max ratio (hi/lo): {:.2}x", hi_avg_max / lo_avg_max);
        }
        if hi_avg_std > 1.2 * lo_avg_std || hi_avg_max > 1.3 * lo_avg_max {
            println!();
            println!("*** CONCLUSION: High-complexity positions have {:.0}% higher eval-swing variance ***",
                     (hi_avg_std / lo_avg_std - 1.0) * 100.0);
            println!("*** and {:.0}% larger maximum single-move eval swings ***",
                     (hi_avg_max / lo_avg_max - 1.0) * 100.0);
            println!();
            println!("A fixed futility margin cannot be optimal across both regime types.");
            println!("Variance-aware pruning that dynamically adjusts margins based on position");
            println!("complexity is empirically justified.");
        }
    }
}
