import {useEffect, useState} from "react";
import {api, pgnUrl} from "../api";
import type {Navigate} from "../App";
import type {MatchDetailResponse} from "../types";
import {Progress} from "./MatchesPage";
import {dateShort, engine1Name, formatElo, formatPercent, labelEngine, perspectiveResult, progress, scoreText, statusClass} from "./helpers";

export function MatchDetail({id, navigate, refresh}: {id: string; navigate: Navigate; refresh: () => void}) {
  const [data, setData] = useState<MatchDetailResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    api.match(id).then((next) => alive && setData(next)).catch((err) => alive && setError(err instanceof Error ? err.message : String(err)));
    const interval = setInterval(() => api.match(id).then((next) => alive && setData(next)).catch(() => undefined), 2500);
    return () => {
      alive = false;
      clearInterval(interval);
    };
  }, [id]);

  const stop = async () => {
    setBusy(true);
    try {
      await api.stopMatch(id);
      refresh();
      setData(await api.match(id));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  if (error) return <section className="panel alert">{error}</section>;
  if (!data) return <section className="panel">Loading match…</section>;
  const match = data.summary;
  const total = Math.max(1, match.results.wins + match.results.draws + match.results.losses);

  return (
    <div className="page-stack">
      <section className="panel match-header">
        <button onClick={() => navigate("/matches")}>← Matches</button>
        <div>
          <p className="eyebrow">{match.id}</p>
          <h1>{match.results.name1 ?? labelEngine(match.white)} vs {match.results.name2 ?? labelEngine(match.black)}</h1>
          <div className="meta-line">
            <span className={`badge ${statusClass(match.status)}`}>{match.status}</span>
            <span>TC {match.settings.tc}</span>
            <span>{match.settings.concurrency} threads</span>
            <span>{match.settings.hash} MB hash</span>
            <span>{match.settings.openings ? "openings" : "no openings"}</span>
            <span>started {dateShort(match.started)}</span>
          </div>
        </div>
        <div className="header-actions">
          <a className="button" href={pgnUrl(id)}>Download PGN</a>
          {match.status === "running" && <button disabled={busy} onClick={stop}>Stop</button>}
        </div>
      </section>

      <section className="card-grid four">
        <div className="metric good"><span>Score</span><strong>{scoreText(match)}</strong></div>
        <div className="metric"><span>Games</span><strong>{match.results.games_done}/{match.settings.games}</strong></div>
        <div className={`metric ${(match.results.elo ?? 0) >= 0 ? "good" : "bad"}`}><span>Elo ± err</span><strong>{formatElo(match.results.elo)} {match.results.elo_error ? `±${match.results.elo_error}` : ""}</strong></div>
        <div className="metric"><span>LOS</span><strong>{formatPercent(match.results.los)}</strong></div>
      </section>

      <section className="panel">
        <div className="section-heading"><h2>Score visualization</h2><Progress value={progress(match)} label={`${progress(match)}% complete`} /></div>
        <div className="stacked-bar" title={scoreText(match)}>
          <div className="wins" style={{width: `${(match.results.wins / total) * 100}%`}} />
          <div className="draws" style={{width: `${(match.results.draws / total) * 100}%`}} />
          <div className="losses" style={{width: `${(match.results.losses / total) * 100}%`}} />
        </div>
        {match.settings.sprt && <p className="muted">SPRT LLR {match.results.sprt_llr ?? "—"} · {match.results.sprt_result ?? "running"}</p>}
      </section>

      <section className="panel">
        <div className="section-heading"><h2>Games</h2><span className="muted">Engine 1: {engine1Name(match)}</span></div>
        <div className="table-wrap">
          <table>
            <thead><tr><th>#</th><th>E1 color</th><th>Result</th><th>Raw</th><th>White</th><th>Black</th><th>Plies</th><th>Termination</th><th></th></tr></thead>
            <tbody>
              {data.games.map((game) => {
                const result = perspectiveResult(match, game);
                return (
                  <tr key={game.index}>
                    <td>{game.index + 1}</td>
                    <td>{game.white === engine1Name(match) ? "White" : "Black"}</td>
                    <td><span className={`badge ${result.tone}`}>{result.label}</span></td>
                    <td>{game.result}</td>
                    <td>{game.white}</td>
                    <td>{game.black}</td>
                    <td>{game.plies}</td>
                    <td>{game.termination}</td>
                    <td><button onClick={() => navigate(`/matches/${id}/games/${game.index}`)}>Replay</button></td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
        {data.games.length === 0 && <p className="muted empty-state">No completed games yet.</p>}
      </section>
    </div>
  );
}
