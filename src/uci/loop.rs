use super::*;
pub fn run() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};

    let z = Zobrist::new();
    let atk = AttackTables::init();
    let mut tt = TranspositionTable::new(128);

    let mut board = Board::startpos();
    let mut position_history: Vec<u64> = Vec::new();
    let mut contempt = 0i32;
    let mut options = EngineOptions::default();
    let mut syzygy: Option<SyzygyTablebase> = None;
    let mut game_id = 0u64;
    let mut search_id = 0u64;

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
                let defaults = EngineOptions::default();
                println!("id name Boa v2.0");
                println!("id author Dirac");
                println!("option name Hash type spin default 128 min 1 max 4096");
                println!("option name Threads type spin default 1 min 1 max 64");
                println!("option name Contempt type spin default 0 min -100 max 100");
                println!("option name SyzygyPath type string default <empty>");
                println!(
                    "option name CriticalityLogDir type string default {}",
                    if defaults.criticality.log_dir.is_empty() {
                        "<empty>"
                    } else {
                        defaults.criticality.log_dir.as_str()
                    }
                );
                println!(
                    "option name CriticalityProbePermille type spin default {} min 0 max 1000",
                    defaults.criticality.probe_permille
                );
                println!(
                    "option name FutilityProbePermille type spin default {} min 0 max 1000",
                    defaults.criticality.futility_probe_permille
                );
                println!(
                    "option name SyzygyProbeDepth type spin default {} min 0 max 64",
                    defaults.syzygy.probe_depth
                );
                println!(
                    "option name SyzygyProbeLimit type spin default {} min 0 max 6",
                    defaults.syzygy.probe_limit
                );
                println!(
                    "option name Syzygy50MoveRule type check default {}",
                    defaults.syzygy.fifty_move_rule
                );
                print_engine_options();
                println!("uciok");
                let _ = io::stdout().flush();
            }
            Some("isready") => {
                println!("readyok");
                let _ = io::stdout().flush();
            }
            Some("ucinewgame") => {
                game_id = game_id.wrapping_add(1);
                board = Board::startpos();
                position_history.clear();
                tt.clear();
            }
            Some("setoption") => {
                handle_setoption(tokens, &mut tt, &mut contempt, &mut options, &mut syzygy);
            }
            Some("position") => {
                handle_position(tokens, &mut board, &mut position_history, &atk, &z);
            }
            Some("go") => {
                search_id = search_id.wrapping_add(1);
                stop_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                handle_go(
                    tokens,
                    GoContext {
                        board: &mut board,
                        position_history: &position_history,
                        atk: &atk,
                        z: &z,
                        tt: &tt,
                        contempt,
                        options: options.clone(),
                        syzygy: syzygy.as_ref(),
                        stop_flag: &stop_flag,
                        game_id,
                        search_id,
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
                let score = evaluate(
                    &board,
                    &EvalContext {
                        atk: &atk,
                        options: &options,
                    },
                );
                println!("eval: {} cp (side to move)", score);
            }
            Some("restriction_features_header") => {
                println!("{}", RestrictionFeatures::csv_header());
                let _ = io::stdout().flush();
            }
            Some("restriction_features") => {
                let features = extract_restriction_features(&board, &atk, &z, options.clone());
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
