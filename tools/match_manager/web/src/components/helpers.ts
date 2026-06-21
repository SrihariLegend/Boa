import type {EngineSpec, GameRow, MatchStatus, MatchSummary} from "../types";

export function labelEngine(engine: EngineSpec): string {
  return engine.type === "stockfish" ? `Stockfish ${engine.elo}` : engine.name;
}

export function formatElo(elo: number | null): string {
  if (elo === null) return "—";
  return `${elo > 0 ? "+" : ""}${elo.toFixed(elo % 1 === 0 ? 0 : 1)}`;
}

export function formatPercent(value: number | null): string {
  if (value === null) return "—";
  return `${value.toFixed(value % 1 === 0 ? 0 : 1)}%`;
}

export function scoreText(match: MatchSummary): string {
  const {wins, draws, losses} = match.results;
  return `+${wins} =${draws} -${losses}`;
}

export function progress(match: MatchSummary): number {
  return Math.min(100, Math.round((match.results.games_done / Math.max(1, match.settings.games)) * 100));
}

export function statusClass(status: MatchStatus): string {
  if (status === "running") return "running";
  if (status === "finished") return "good";
  if (status === "error" || status === "interrupted") return "bad";
  if (status === "stopped") return "neutral";
  return "warn";
}

export function engine1Name(match: MatchSummary): string {
  return match.results.name1 ?? labelEngine(match.white);
}

export function perspectiveResult(match: MatchSummary, game: GameRow): {label: string; tone: "good" | "bad" | "neutral" | "warn"} {
  if (game.result === "*") return {label: "Pending", tone: "warn"};
  if (game.result === "1/2-1/2") return {label: "Draw", tone: "neutral"};
  const e1 = engine1Name(match);
  const e1White = game.white === e1;
  const e1Won = (game.result === "1-0" && e1White) || (game.result === "0-1" && !e1White);
  return e1Won ? {label: "Win", tone: "good"} : {label: "Loss", tone: "bad"};
}

export function dateShort(value: string | null | undefined): string {
  if (!value) return "—";
  return value.replace("T", " ");
}
