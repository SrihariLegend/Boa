import type {ReactNode} from "react";
import type {Navigate} from "../App";
import type {EngineMeta, MatchSummary} from "../types";

export function Layout({children, path, navigate, matches, engines, error}: {
  children: ReactNode;
  path: string;
  navigate: Navigate;
  matches: MatchSummary[];
  engines: EngineMeta[];
  error: string | null;
}) {
  const running = matches.filter((match) => match.status === "running").length;
  const games = matches.reduce((sum, match) => sum + match.results.games_done, 0);
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">♛</div>
          <div>
            <strong>Boa</strong>
            <span>Match Manager</span>
          </div>
        </div>
        <nav>
          <NavButton active={path === "/"} onClick={() => navigate("/")}>Dashboard</NavButton>
          <NavButton active={path.startsWith("/matches") && path !== "/matches/new"} onClick={() => navigate("/matches")}>Matches</NavButton>
          <NavButton active={path === "/matches/new"} onClick={() => navigate("/matches/new")}>New Match</NavButton>
          <NavButton active={path === "/engines"} onClick={() => navigate("/engines")}>Engines</NavButton>
        </nav>
      </aside>
      <main className="main">
        <header className="topbar">
          <div>
            <strong>{running} running</strong>
            <span>{games} games tracked</span>
            <span>{engines.length} snapshots</span>
          </div>
          <div className="server-pill">local · 127.0.0.1</div>
        </header>
        {error && <div className="alert">API error: {error}</div>}
        {children}
      </main>
    </div>
  );
}

function NavButton({active, onClick, children}: {active: boolean; onClick: () => void; children: ReactNode}) {
  return <button className={active ? "nav-link active" : "nav-link"} onClick={onClick}>{children}</button>;
}
