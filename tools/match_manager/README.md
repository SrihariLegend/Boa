# Karpov Match Manager

Terminal UI for engine approval matches.

```sh
cd tools/match_manager
npm install
npm run build
./match-manager
```

The CLI uses the same persistent data directories as the old web UI:

- `engines/` for saved engine snapshots
- `matches/` for match configs, status, logs, and PGNs

Main features:

- Snapshot the current `cargo build --release` engine.
- Import an existing Karpov binary.
- Delete unused snapshots.
- Start cutechess matches against snapshots or Stockfish.
- Configure games, time control, concurrency, hash, openings, adjudication, SPRT, and UCI options.
- Monitor running matches with live Elo, LOS, score, and SPRT state.
- Stop or delete matches.
- Browse PGN games and replay them on a terminal chessboard.
