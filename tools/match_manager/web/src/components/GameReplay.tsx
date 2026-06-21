import {Fragment, useEffect, useMemo, useState} from "react";
import {Chessboard} from "react-chessboard";
import {api, pgnUrl} from "../api";
import type {Navigate} from "../App";
import type {GameDetail, MatchDetailResponse} from "../types";
import {perspectiveResult} from "./helpers";

export function GameReplay({id, index, navigate}: {id: string; index: number; navigate: Navigate}) {
  const [match, setMatch] = useState<MatchDetailResponse | null>(null);
  const [game, setGame] = useState<GameDetail | null>(null);
  const [ply, setPly] = useState(0);
  const [flipped, setFlipped] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    Promise.all([api.match(id), api.game(id, index)])
      .then(([nextMatch, nextGame]) => {
        if (!alive) return;
        setMatch(nextMatch);
        setGame(nextGame);
        setPly(0);
        setError(null);
      })
      .catch((err) => alive && setError(err instanceof Error ? err.message : String(err)));
    return () => {
      alive = false;
    };
  }, [id, index]);

  const maxPly = Math.max(0, (game?.fens.length ?? 1) - 1);
  const clamp = (next: number) => setPly(Math.max(0, Math.min(maxPly, next)));

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (["INPUT", "TEXTAREA", "SELECT"].includes((event.target as HTMLElement | null)?.tagName ?? "")) return;
      if (event.key === "ArrowLeft" || event.key === "h") clamp(ply - 1);
      else if (event.key === "ArrowRight" || event.key === "l") clamp(ply + 1);
      else if (event.key === "ArrowUp" || event.key === "k") clamp(ply - 10);
      else if (event.key === "ArrowDown" || event.key === "j") clamp(ply + 10);
      else if (event.key === "Home") clamp(0);
      else if (event.key === "End") clamp(maxPly);
      else if (event.key === "n") navigate(`/matches/${id}/games/${index + 1}`);
      else if (event.key === "p" && index > 0) navigate(`/matches/${id}/games/${index - 1}`);
      else return;
      event.preventDefault();
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, [id, index, maxPly, navigate, ply]);

  const row = match?.games.find((candidate) => candidate.index === index);
  const result = match && row ? perspectiveResult(match.summary, row) : null;
  const fen = game?.fens[ply] ?? "8/8/8/8/8/8/8/8 w - - 0 1";
  const moves = useMemo(() => pairMoves(game?.sans ?? []), [game]);

  if (error) return <section className="panel alert">{error}</section>;
  if (!game || !match) return <section className="panel">Loading game…</section>;

  return (
    <div className="page-stack replay-page">
      <section className="panel match-header">
        <button onClick={() => navigate(`/matches/${id}`)}>← Match</button>
        <div>
          <p className="eyebrow">Game {index + 1}</p>
          <h1>{game.headers.White ?? "White"} vs {game.headers.Black ?? "Black"}</h1>
          <div className="meta-line">
            {result && <span className={`badge ${result.tone}`}>{result.label}</span>}
            <span>{game.headers.Result ?? "*"}</span>
            <span>{game.headers.Termination ?? "normal"}</span>
            <span>Ply {ply}/{maxPly}</span>
          </div>
        </div>
        <div className="header-actions">
          <a className="button" href={pgnUrl(id, index)}>PGN</a>
          <button onClick={() => setFlipped((value) => !value)}>{flipped ? "White bottom" : "Black bottom"}</button>
        </div>
      </section>

      <section className="replay-grid">
        <div className="panel board-panel">
          <ChessBoard fen={fen} flipped={flipped} />
          <div className="replay-controls">
            <button onClick={() => clamp(0)}>Start</button>
            <button onClick={() => clamp(ply - 10)}>-10</button>
            <button onClick={() => clamp(ply - 1)}>Prev</button>
            <button onClick={() => clamp(ply + 1)}>Next</button>
            <button onClick={() => clamp(ply + 10)}>+10</button>
            <button onClick={() => clamp(maxPly)}>End</button>
          </div>
          <p className="muted shortcuts">←/h prev · →/l next · ↑/k -10 · ↓/j +10 · Home/End · n/p game</p>
        </div>

        <aside className="panel move-panel">
          <h2>Moves</h2>
          <div className="move-list">
            {moves.map((move) => (
              <div className="move-row" key={move.number}>
                <span className="move-number">{move.number}.</span>
                <button className={ply === move.whitePly ? "active-move" : ""} onClick={() => clamp(move.whitePly)}>{move.white}</button>
                {move.black && <button className={ply === move.blackPly ? "active-move" : ""} onClick={() => clamp(move.blackPly)}>{move.black}</button>}
              </div>
            ))}
          </div>
          <h2>Metadata</h2>
          <dl className="metadata">
            {Object.entries(game.headers).map(([key, value]) => <Fragment key={key}><dt>{key}</dt><dd>{value}</dd></Fragment>)}
          </dl>
        </aside>
      </section>
    </div>
  );
}

function ChessBoard({fen, flipped}: {fen: string; flipped: boolean}) {
  return (
    <div className="chessboard-frame">
      <Chessboard
        options={{
          id: "boa-replay-board",
          position: fen,
          boardOrientation: flipped ? "black" : "white",
          allowDragging: false,
          allowDrawingArrows: false,
          showNotation: true,
          animationDurationInMs: 120,
          boardStyle: {borderRadius: "12px", boxShadow: "none"},
          lightSquareStyle: {backgroundColor: "#d6e3d1"},
          darkSquareStyle: {backgroundColor: "#5f7f9c"},
        }}
      />
    </div>
  );
}

function pairMoves(sans: string[]): {number: number; white: string; whitePly: number; black?: string; blackPly: number}[] {
  const pairs = [];
  for (let i = 0; i < sans.length; i += 2) {
    pairs.push({number: i / 2 + 1, white: sans[i], whitePly: i + 1, black: sans[i + 1], blackPly: i + 2});
  }
  return pairs;
}
