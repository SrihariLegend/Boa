use super::*;
pub(in crate::search) const BENCH_FENS: [&str; 20] = [
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "r1bq1rk1/pp2ppbp/2n3p1/2pp4/5P2/2NP1NP1/PPP1P1BP/R1BQ1RK1 w - - 0 8",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    "r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
    "r1bqk2r/ppppbppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQ1RK1 w kq - 6 5",
    "r1bq1rk1/ppp2ppp/2np1n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 w - - 0 7",
    "r2qkbnr/ppp2ppp/2n1b3/3pp3/4P3/2N2N2/PPPP1PPP/R1BQKB1R w KQkq - 4 4",
    "rnbqk2r/ppp1bppp/4pn2/3p4/2PP4/2N2N2/PP2PPPP/R1BQKB1R w KQkq - 2 4",
    "r1bqkb1r/pp3ppp/2n1pn2/2pp4/2PP4/2N2N2/PP2PPPP/R1BQKB1R w KQkq - 0 5",
    "r1bqk2r/pp2bppp/2n1pn2/3p4/2PP4/2N2N2/PP2PPPP/R1BQ1RK1 b kq - 5 6",
    "rnbq1rk1/ppp1bppp/4pn2/3p4/2PP4/5NP1/PP2PPBP/RNBQK2R w KQ - 4 5",
    "r1bq1rk1/pppn1ppp/4pn2/3p4/1bPP4/2N2NP1/PP2PPBP/R1BQK2R w KQ - 2 6",
    "2r3k1/pp2ppbp/6p1/8/2Bn4/2N3P1/PP2PP1P/3R2K1 w - - 0 1",
    "r4rk1/pp2ppbp/2np2p1/q7/4P3/2N1BP2/PPPQ2PP/R4RK1 w - - 0 1",
    "r2q1rk1/pb1nbppp/1pp1pn2/3p4/2PP4/1PN1PN2/PB2BPPP/R2Q1RK1 w - - 0 9",
    "r1b2rk1/2q1bppp/p2ppn2/1p6/3QP3/1BN1BP2/PPP3PP/R4RK1 w - - 0 12",
    "2rr2k1/1p3ppp/p1nbbp2/4p3/4P3/1NN1B1P1/PP3PBP/R4RK1 w - - 0 1",
];

pub fn bench(atk: &AttackTables, z: &Zobrist, depth: u32) {
    use crate::tt::TranspositionTable;
    let mut tt = TranspositionTable::new(64);
    let mut total_nodes = 0u64;
    let start = now_ms();

    let no_stop = std::sync::atomic::AtomicBool::new(false);
    for (i, fen) in BENCH_FENS.iter().enumerate() {
        let mut board = Board::from_fen(fen).unwrap();
        let limits = Limits {
            max_depth: depth,
            move_time: 10_000,
            ..Limits::default()
        };
        let mut ctx = SearchContext::new(
            atk,
            z,
            &mut tt,
            limits,
            Vec::new(),
            20,
            EngineOptions::default(),
            None,
            &no_stop,
            0,
            i as u64 + 1,
        );
        let result = search(&mut board, &mut ctx);
        total_nodes += result.nodes;
        eprintln!(
            "bench {}/{}: {} nodes  score {} pv {}",
            i + 1,
            BENCH_FENS.len(),
            result.nodes,
            result.score,
            move_name(result.best_move)
        );
        tt.clear();
    }

    let elapsed = (now_ms() - start).max(1);
    let nps = total_nodes * 1000 / elapsed;
    eprintln!("============================");
    eprintln!("Total time  : {} ms", elapsed);
    eprintln!("Total nodes : {}", total_nodes);
    eprintln!("Nodes/sec   : {}", nps);
    eprintln!("Positions   : {}", BENCH_FENS.len());
    eprintln!("Depth       : {}", depth);
    println!("{}", total_nodes);
}

// ============================================================
// Section 1: Iterative deepening
