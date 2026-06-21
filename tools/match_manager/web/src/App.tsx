import {useEffect, useMemo, useState} from "react";
import {api} from "./api";
import {Dashboard} from "./components/Dashboard";
import {EngineLibrary} from "./components/EngineLibrary";
import {Layout} from "./components/Layout";
import {MatchDetail} from "./components/MatchDetail";
import {MatchesPage} from "./components/MatchesPage";
import {NewMatchForm} from "./components/NewMatchForm";
import {GameReplay} from "./components/GameReplay";
import type {EngineMeta, MatchSummary} from "./types";

export type Navigate = (path: string) => void;

export function App() {
  const [path, setPath] = useState(location.pathname);
  const [tick, setTick] = useState(0);
  const [matches, setMatches] = useState<MatchSummary[]>([]);
  const [engines, setEngines] = useState<EngineMeta[]>([]);
  const [error, setError] = useState<string | null>(null);

  const navigate: Navigate = (next) => {
    history.pushState(null, "", next);
    setPath(next);
  };

  useEffect(() => {
    const onPop = () => setPath(location.pathname);
    addEventListener("popstate", onPop);
    return () => removeEventListener("popstate", onPop);
  }, []);

  useEffect(() => {
    const events = new EventSource("/api/events");
    events.onmessage = () => setTick((value) => value + 1);
    events.onerror = () => undefined;
    return () => events.close();
  }, []);

  useEffect(() => {
    let alive = true;
    Promise.all([api.matches(), api.engines()])
      .then(([nextMatches, nextEngines]) => {
        if (!alive) return;
        setMatches(nextMatches);
        setEngines(nextEngines);
        setError(null);
      })
      .catch((err: unknown) => alive && setError(err instanceof Error ? err.message : String(err)));
    return () => {
      alive = false;
    };
  }, [tick]);

  const route = useMemo(() => parseRoute(path), [path]);
  const refresh = () => setTick((value) => value + 1);

  return (
    <Layout path={path} navigate={navigate} matches={matches} engines={engines} error={error}>
      {route.kind === "dashboard" && <Dashboard matches={matches} engines={engines} navigate={navigate} />}
      {route.kind === "matches" && <MatchesPage matches={matches} navigate={navigate} refresh={refresh} />}
      {route.kind === "match" && <MatchDetail id={route.id} navigate={navigate} refresh={refresh} />}
      {route.kind === "game" && <GameReplay id={route.id} index={route.index} navigate={navigate} />}
      {route.kind === "new" && <NewMatchForm engines={engines} navigate={navigate} refresh={refresh} />}
      {route.kind === "engines" && <EngineLibrary engines={engines} refresh={refresh} />}
      {route.kind === "missing" && <Missing navigate={navigate} />}
    </Layout>
  );
}

function Missing({navigate}: {navigate: Navigate}) {
  return (
    <section className="panel empty-state">
      <h1>Page not found</h1>
      <button className="primary" onClick={() => navigate("/")}>Back to dashboard</button>
    </section>
  );
}

type Route =
  | {kind: "dashboard"}
  | {kind: "matches"}
  | {kind: "match"; id: string}
  | {kind: "game"; id: string; index: number}
  | {kind: "new"}
  | {kind: "engines"}
  | {kind: "missing"};

function parseRoute(path: string): Route {
  if (path === "/") return {kind: "dashboard"};
  if (path === "/matches") return {kind: "matches"};
  if (path === "/matches/new") return {kind: "new"};
  if (path === "/engines") return {kind: "engines"};
  const game = /^\/matches\/([^/]+)\/games\/(\d+)$/.exec(path);
  if (game) return {kind: "game", id: decodeURIComponent(game[1]), index: Number(game[2])};
  const match = /^\/matches\/([^/]+)$/.exec(path);
  if (match) return {kind: "match", id: decodeURIComponent(match[1])};
  return {kind: "missing"};
}
