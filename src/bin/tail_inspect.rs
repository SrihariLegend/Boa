/// Tail inspection: dump every false-prune for manual classification.
///
/// Runs at z=0.8 (most failures) with depth-4 verification ground truth.
/// For each failure prints FEN, move, σ, required_gain, true_gain, and
/// the static evals before/after the move — enough to classify the motif.
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

    const Z: f64 = 0.8;
    const VERIFY_DEPTH: i32 = 4;
    const REQUIRED_GAINS: [i32; 4] = [15, 25, 40, 60];

    println!("=== Tail Inspection: z={:.1}, verification depth={} ===", Z, VERIFY_DEPTH);
    println!();

    let mut failures: Vec<(String, String, i32, i32, i32, i32, i32, i32)> = Vec::new();
    // (fen, move_uci, sigma, required_gain, var_margin, true_gain, eval_before, eval_after_opp)

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
            let eval_after_opp = eval::evaluate(&child, &ectx);
            child.unmake_move(m, &undo, &zobrist);

            for &rg in &REQUIRED_GAINS {
                let var_margin = (Z * s as f64).round() as i32;
                let var_prunes = var_margin < rg;
                let safe = true_gain < rg;

                if var_prunes && !safe {
                    failures.push((
                        fen.to_string(),
                        move_name(m),
                        s, rg, var_margin, true_gain,
                        eval_before, eval_after_opp,
                    ));
                }
            }
        }
    }

    println!("Total false prunes at z={:.1}: {}", Z, failures.len());
    println!();

    // Summarize by category proxies
    let mut by_sigma: std::collections::BTreeMap<i32, u32> = std::collections::BTreeMap::new();
    let mut by_missed: Vec<i32> = Vec::new();
    for (_, _, s, rg, _, tg, _, _) in &failures {
        *by_sigma.entry(*s).or_default() += 1;
        by_missed.push(tg - rg);
    }
    by_missed.sort_unstable();

    println!("By σ:");
    for (s, c) in &by_sigma { println!("  σ={}: {} failures", s, c); }
    println!();
    println!("Missed-by distribution: min={}  med={}  p95={}  max={}",
             by_missed.first().unwrap_or(&0),
             by_missed[by_missed.len()/2],
             by_missed[(by_missed.len()*95/100).min(by_missed.len()-1)],
             by_missed.last().unwrap_or(&0));
    println!();
    println!("{}", "=".repeat(80));
    println!("INDIVIDUAL FAILURES");
    println!("{:<55} {:<6} {:<6} {:>4} {:>4} {:>5} {:>5} {:>5}",
             "FEN", "Move", "σ", "rg", "M", "Δtrue", "eval", "ch_ev");
    println!("{}", "-".repeat(95));

    for (fen, m, s, rg, margin, true_gain, ev_before, ev_after) in &failures {
        // Truncate FEN for display
        let short_fen = if fen.len() > 52 { &fen[..52] } else { fen };
        println!("{:<55} {:<6} {:<6} {:>4} {:>4} {:>5} {:>5} {:>5}",
                 short_fen, m, s, rg, margin, true_gain, ev_before, ev_after);
    }
}
