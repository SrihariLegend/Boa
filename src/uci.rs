// ============================================================
// uci.rs — UCI protocol handler
// ============================================================

use crate::board::{Board, Zobrist};
use crate::config::EngineOptions;
use crate::diagnostics::{extract_restriction_features, RestrictionFeatures};
use crate::movegen::{perft, AttackTables};
use crate::search::{search, Limits, SearchContext};
use crate::tt::TranspositionTable;
use crate::types::*;
use std::io::{self, BufRead, Write};

fn handle_setoption<'a>(
    tokens: impl Iterator<Item = &'a str>,
    tt: &mut TranspositionTable,
    contempt: &mut i32,
    options: &mut EngineOptions,
) {
    let mut name_parts = Vec::new();
    let mut value_parts = Vec::new();
    let mut reading_name = false;
    let mut reading_value = false;
    for tok in tokens {
        match tok {
            "name" => {
                reading_name = true;
                reading_value = false;
            }
            "value" => {
                reading_value = true;
                reading_name = false;
            }
            t => {
                if reading_name {
                    name_parts.push(t);
                }
                if reading_value {
                    value_parts.push(t);
                }
            }
        }
    }
    let name = name_parts.join(" ");
    let val = value_parts.join(" ");
    let name_key = name.to_ascii_lowercase().replace(' ', "");
    match name_key.as_str() {
        "hash" => {
            let mb: usize = val.parse().unwrap_or(128);
            *tt = TranspositionTable::new(mb.clamp(1, 4096));
        }
        "contempt" => {
            *contempt = val.parse().unwrap_or(20);
        }
        _ => {
            let _ = options.set_uci_option(&name, &val);
        }
    }
}

fn handle_position<'a>(
    mut tokens: impl Iterator<Item = &'a str>,
    board: &mut Board,
    position_history: &mut Vec<u64>,
    atk: &AttackTables,
    z: &Zobrist,
) {
    position_history.clear();
    match tokens.next() {
        Some("startpos") => {
            *board = Board::startpos();
        }
        Some("fen") => {
            let fen = collect_fen(&mut tokens);
            *board = Board::from_fen(&fen).unwrap_or_else(Board::startpos);
        }
        _ => {}
    }
    position_history.push(board.hash);
    for tok in tokens {
        let Some(m) = move_from_uci(tok) else {
            continue;
        };
        let Some(lm) = find_legal_move(board, atk, z, m) else {
            continue;
        };
        let _undo = board.make_move(lm, z);
        position_history.push(board.hash);
    }
}

fn collect_fen<'a>(tokens: &mut impl Iterator<Item = &'a str>) -> String {
    let mut fen_parts = Vec::new();
    for tok in tokens {
        if tok == "moves" {
            break;
        }
        fen_parts.push(tok);
    }
    fen_parts.join(" ")
}

struct GoContext<'a> {
    board: &'a mut Board,
    position_history: &'a [u64],
    atk: &'a AttackTables,
    z: &'a Zobrist,
    tt: &'a mut TranspositionTable,
    contempt: i32,
    options: EngineOptions,
    stop_flag: &'a std::sync::atomic::AtomicBool,
}

fn handle_go<'a>(tokens: impl Iterator<Item = &'a str>, go: GoContext<'_>) {
    let mut limits = Limits::default();
    let mut tok_iter = tokens.peekable();
    while let Some(tok) = tok_iter.next() {
        match tok {
            "depth" => {
                limits.max_depth = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(64);
            }
            "nodes" => {
                limits.nodes = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "movetime" => {
                limits.move_time = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "wtime" => {
                limits.wtime = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "btime" => {
                limits.btime = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "winc" => {
                limits.winc = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "binc" => {
                limits.binc = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "movestogo" => {
                limits.moves_to_go = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            }
            "infinite" => {
                limits.max_depth = 64;
            }
            "perft" => {
                let d: u32 = tok_iter.next().and_then(|t| t.parse().ok()).unwrap_or(1);
                run_perft(go.board, go.atk, go.z, d);
                return;
            }
            _ => {}
        }
    }
    let history_for_search =
        go.position_history[..go.position_history.len().saturating_sub(1)].to_vec();
    let mut ctx = SearchContext::new(
        go.atk,
        go.z,
        go.tt,
        limits,
        history_for_search,
        go.contempt,
        go.options,
        go.stop_flag,
    );
    let result = search(go.board, &mut ctx);
    println!("bestmove {}", move_name(result.best_move));
    let _ = io::stdout().flush();
}

pub fn run() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};

    let z = Zobrist::new();
    let atk = AttackTables::init();
    let mut tt = TranspositionTable::new(128);

    let mut board = Board::startpos();
    let mut position_history: Vec<u64> = Vec::new();
    let mut contempt = 20i32; // draw avoidance — Boa never seeks draws (positive = avoid draws for root side)
    let mut options = EngineOptions::default();

    // Input thread: the search blocks the main thread, so "stop"/"quit" must
    // be seen by a reader thread that flips the stop flag immediately.
    // Otherwise a search that outlives its game emits a stale bestmove into
    // the NEXT game (scored by the GUI as an illegal move).
    let stop_flag = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<String>();
    {
        let stop_flag = Arc::clone(&stop_flag);
        std::thread::spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                let Ok(line) = line else { break };
                let t = line.trim();
                if t == "stop" || t == "quit" {
                    stop_flag.store(true, Ordering::Relaxed);
                }
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }

    // Don't print UCI info at startup — wait for the "uci" command per protocol spec

    for line in rx {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut tokens = line.split_whitespace();
        match tokens.next() {
            Some("uci") => {
                println!("id name Boa v2.0");
                println!("id author Dirac");
                println!("option name Hash type spin default 128 min 1 max 4096");
                println!("option name Contempt type spin default 20 min -100 max 100");
                print_engine_options();
                println!("uciok");
                let _ = io::stdout().flush();
            }
            Some("isready") => {
                println!("readyok");
                let _ = io::stdout().flush();
            }
            Some("ucinewgame") => {
                board = Board::startpos();
                position_history.clear();
                tt.clear();
            }
            Some("setoption") => {
                handle_setoption(tokens, &mut tt, &mut contempt, &mut options);
            }
            Some("position") => {
                handle_position(tokens, &mut board, &mut position_history, &atk, &z);
            }
            Some("go") => {
                stop_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                handle_go(
                    tokens,
                    GoContext {
                        board: &mut board,
                        position_history: &position_history,
                        atk: &atk,
                        z: &z,
                        tt: &mut tt,
                        contempt,
                        options,
                        stop_flag: &stop_flag,
                    },
                );
            }
            Some("stop") => {
                // Search already returned by the time this is dequeued;
                // the input thread set the flag when it arrived.
            }
            Some("quit") => break,
            Some("d") | Some("display") => {
                board.display();
            }
            Some("perft") => {
                let d: u32 = tokens.next().and_then(|t| t.parse().ok()).unwrap_or(1);
                run_perft(&mut board, &atk, &z, d);
            }
            Some("eval") => {
                use crate::eval::{evaluate, EvalContext};
                let score = evaluate(&board, &EvalContext { atk: &atk, options });
                println!("eval: {} cp (side to move)", score);
            }
            Some("restriction_features_header") => {
                println!("{}", RestrictionFeatures::csv_header());
                let _ = io::stdout().flush();
            }
            Some("restriction_features") => {
                let features = extract_restriction_features(&board, &atk, &z, options);
                println!("{}", features.to_csv_row());
                let _ = io::stdout().flush();
            }
            Some("bench") => {
                let depth: u32 = tokens.next().and_then(|t| t.parse().ok()).unwrap_or(10);
                crate::search::bench(&atk, &z, depth);
            }
            _ => {}
        }
    }
}

fn print_engine_options() {
    let defaults = EngineOptions::default();
    println!(
        "option name Search Restriction Ordering Scale type spin default {} min 0 max 300",
        defaults.search.restriction_ordering_scale
    );

    for (name, default) in [
        ("Eval Material Scale", defaults.eval.material_scale),
        ("Eval PST Scale", defaults.eval.pst_scale),
        ("Eval Mobility Scale", defaults.eval.mobility_scale),
        (
            "Eval Pawn Structure Scale",
            defaults.eval.pawn_structure_scale,
        ),
        ("Eval King Safety Scale", defaults.eval.king_safety_scale),
        ("Eval Freedom Scale", defaults.eval.freedom_scale),
        ("Eval Trade Down Scale", defaults.eval.trade_down_scale),
        ("Eval Weak Squares Scale", defaults.eval.weak_squares_scale),
        ("Eval Coordination Scale", defaults.eval.coordination_scale),
        (
            "Eval Advanced Pawns Scale",
            defaults.eval.advanced_pawns_scale,
        ),
    ] {
        println!(
            "option name {} type spin default {} min 0 max 300",
            name, default
        );
    }
    for name in [
        "Search Restriction Ordering",
        "Search Squeeze Extensions",
        "Search Squeeze Null Move Suppression",
        "Search Squeeze LMR Relief",
    ] {
        println!("option name {} type check default true", name);
    }
}

fn is_legal_move(board: &Board, z: &Zobrist, lm: Move) -> bool {
    let mut b = board.clone();
    let _undo = b.make_move(lm, z);
    !b.is_in_check(b.side.flip())
}

fn find_legal_move(board: &Board, atk: &AttackTables, z: &Zobrist, m: Move) -> Option<Move> {
    use crate::movegen::gen_moves;
    let from = move_from(m);
    let to = move_to(m);
    let promo_flag = move_flags(m) == MF_PROMOTION;

    let list = gen_moves(board, atk);
    for &lm in list.iter() {
        if move_from(lm) != from || move_to(lm) != to {
            continue;
        }
        if promo_flag && (move_flags(lm) != MF_PROMOTION || move_promo_pt(lm) != move_promo_pt(m)) {
            continue;
        }
        if is_legal_move(board, z, lm) {
            return Some(lm);
        }
    }
    None
}

fn run_perft(board: &mut Board, atk: &AttackTables, z: &Zobrist, depth: u32) {
    use std::time::Instant;
    let start = Instant::now();
    if depth == 0 {
        println!("perft(0) = 1 nodes in 0ms");
        return;
    }
    let list = crate::movegen::gen_moves(board, atk);
    let mut total = 0u64;
    for i in 0..list.count {
        let m = list.moves[i];
        let undo = board.make_move(m, z);
        if board.is_in_check(board.side.flip()) {
            board.unmake_move(m, &undo, z);
            continue;
        }
        let n = perft(board, atk, z, depth - 1);
        board.unmake_move(m, &undo, z);
        if depth >= 2 {
            println!("{}: {}", move_name(m), n);
        }
        total += n;
    }
    let elapsed = start.elapsed().as_millis().max(1) as u64;
    println!(
        "perft({}) = {} nodes in {}ms ({} nps)",
        depth,
        total,
        elapsed,
        total * 1000 / elapsed
    );
}
