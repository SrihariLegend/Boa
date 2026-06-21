import type {Navigate} from "../App";
import type {EngineMeta, MatchSummary} from "../types";
import {formatElo, labelEngine, scoreText, statusClass} from "./helpers";

export function Dashboard({matches, engines, navigate}: {matches: MatchSummary[]; engines: EngineMeta[]; navigate: Navigate}) {
  const running = matches.filter((match) => match.status === "running").length;
  const finished = matches.filter((match) => match.status === "finished").length;
  const games = matches.reduce((sum, match) => sum + match.results.games_done, 0);
  const latest = matches[0];
  const averageElo = average(matches.map((match) => match.results.elo).filter((elo): elo is number => elo !== null));

  return (
    <div className="page-stack">
      <section className="hero">
        <div>
          <p className="eyebrow">Local engine testing</p>
          <h1>Boa Match Dashboard</h1>
          <p>Manage snapshots, run matches, inspect live results, and replay games without leaving localhost.</p>
        </div>
        <div className="hero-actions">
          <button className="primary" onClick={() => navigate("/matches/new")}>New Match</button>
          <button onClick={() => navigate("/engines")}>Engine Library</button>
        </div>
      </section>

      <section className="card-grid four">
        <Metric title="Running matches" value={running} tone="warn" />
        <Metric title="Finished matches" value={finished} />
        <Metric title="Games completed" value={games} />
        <Metric title="Average Elo Δ" value={averageElo === null ? "—" : formatElo(averageElo)} tone={(averageElo ?? 0) >= 0 ? "good" : "bad"} />
      </section>

      <section className="panel">
        <div className="section-heading">
          <h2>Recent matches</h2>
          <button onClick={() => navigate("/matches")}>View all</button>
        </div>
        {matches.length === 0 ? <p className="muted">No matches yet.</p> : (
          <div className="recent-list">
            {matches.slice(0, 6).map((match) => (
              <button className="recent-row" key={match.id} onClick={() => navigate(`/matches/${match.id}`)}>
                <span className={`badge ${statusClass(match.status)}`}>{match.status}</span>
                <strong>{labelEngine(match.white)} vs {labelEngine(match.black)}</strong>
                <span>{scoreText(match)}</span>
                <span>{formatElo(match.results.elo)}</span>
              </button>
            ))}
          </div>
        )}
      </section>

      {latest && (
        <section className="panel">
          <h2>Latest result</h2>
          <p className="big-line">{labelEngine(latest.white)} vs {labelEngine(latest.black)} · {scoreText(latest)} · {formatElo(latest.results.elo)}</p>
        </section>
      )}
    </div>
  );
}

function Metric({title, value, tone}: {title: string; value: string | number; tone?: "good" | "bad" | "warn"}) {
  return <div className={`metric ${tone ?? ""}`}><span>{title}</span><strong>{value}</strong></div>;
}

function average(values: number[]): number | null {
  if (values.length === 0) return null;
  return Math.round((values.reduce((sum, value) => sum + value, 0) / values.length) * 10) / 10;
}
