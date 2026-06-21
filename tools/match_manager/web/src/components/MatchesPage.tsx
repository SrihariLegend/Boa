import {useMemo, useState} from "react";
import {api, pgnUrl} from "../api";
import type {Navigate} from "../App";
import type {MatchStatus, MatchSummary} from "../types";
import {dateShort, formatElo, formatPercent, labelEngine, progress, scoreText, statusClass} from "./helpers";

export function MatchesPage({matches, navigate, refresh}: {matches: MatchSummary[]; navigate: Navigate; refresh: () => void}) {
  const [filter, setFilter] = useState<"all" | MatchStatus>("all");
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const filtered = useMemo(() => matches.filter((match) => {
    const haystack = `${match.id} ${labelEngine(match.white)} ${labelEngine(match.black)} ${match.results.name1 ?? ""} ${match.results.name2 ?? ""}`.toLowerCase();
    return (filter === "all" || match.status === filter) && haystack.includes(search.toLowerCase());
  }), [matches, filter, search]);

  const stop = async (id: string) => action(id, async () => api.stopMatch(id));
  const remove = async (id: string) => {
    if (!confirm(`Delete match ${id}?`)) return;
    await action(id, async () => api.deleteMatch(id));
  };
  const action = async (id: string, fn: () => Promise<unknown>) => {
    setBusy(id);
    setError(null);
    try {
      await fn();
      refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="panel page-stack">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Results</p>
          <h1>Matches</h1>
        </div>
        <button className="primary" onClick={() => navigate("/matches/new")}>New Match</button>
      </div>
      <div className="toolbar">
        <input placeholder="Search engine or match id" value={search} onChange={(event) => setSearch(event.target.value)} />
        <select value={filter} onChange={(event) => setFilter(event.target.value as "all" | MatchStatus)}>
          <option value="all">All statuses</option>
          <option value="running">Running</option>
          <option value="finished">Finished</option>
          <option value="stopped">Stopped</option>
          <option value="interrupted">Interrupted</option>
          <option value="error">Error</option>
        </select>
      </div>
      {error && <div className="alert">{error}</div>}
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Status</th><th>Match ID</th><th>Engine 1</th><th>Engine 2</th><th>Score</th><th>Games</th><th>Elo</th><th>LOS</th><th>TC</th><th>Started</th><th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((match) => (
              <tr key={match.id}>
                <td><span className={`badge ${statusClass(match.status)}`}>{match.status}</span></td>
                <td><button className="link" onClick={() => navigate(`/matches/${match.id}`)}>{match.id}</button></td>
                <td>{match.results.name1 ?? labelEngine(match.white)}</td>
                <td>{match.results.name2 ?? labelEngine(match.black)}</td>
                <td>{scoreText(match)}</td>
                <td><Progress value={progress(match)} label={`${match.results.games_done}/${match.settings.games}`} /></td>
                <td className={(match.results.elo ?? 0) >= 0 ? "good-text" : "bad-text"}>{formatElo(match.results.elo)}</td>
                <td>{formatPercent(match.results.los)}</td>
                <td>{match.settings.tc}</td>
                <td>{dateShort(match.started)}</td>
                <td className="actions">
                  <button onClick={() => navigate(`/matches/${match.id}`)}>Open</button>
                  <a className="button" href={pgnUrl(match.id)}>PGN</a>
                  {match.status === "running" && <button disabled={busy === match.id} onClick={() => stop(match.id)}>Stop</button>}
                  {match.status !== "running" && <button disabled={busy === match.id} onClick={() => remove(match.id)}>Delete</button>}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {filtered.length === 0 && <p className="muted empty-state">No matches found.</p>}
    </section>
  );
}

export function Progress({value, label}: {value: number; label?: string}) {
  return <div className="progress"><div style={{width: `${value}%`}} /><span>{label ?? `${value}%`}</span></div>;
}
