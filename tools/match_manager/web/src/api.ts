import type {EngineMeta, MatchConfig, MatchDetailResponse, MatchSummary, GameDetail, GameRow} from "./types";

export class ApiError extends Error {
  constructor(readonly status: number, message: string) {
    super(message);
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: {"Content-Type": "application/json", ...init?.headers},
  });
  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`;
    try {
      const body = await response.json() as {error?: string};
      if (body.error) message = body.error;
    } catch {
      // ignore non-json errors
    }
    throw new ApiError(response.status, message);
  }
  return await response.json() as T;
}

export const api = {
  health: () => request<{ok: boolean; version: string}>("/api/health"),
  engines: () => request<EngineMeta[]>("/api/engines"),
  snapshotEngine: (name: string, note: string) => request<EngineMeta>("/api/engines/snapshot", {
    method: "POST",
    body: JSON.stringify({name, note}),
  }),
  importEngine: (name: string, binary: string, note: string) => request<EngineMeta>("/api/engines/import", {
    method: "POST",
    body: JSON.stringify({name, binary, note}),
  }),
  deleteEngine: (name: string) => request<{ok: true}>(`/api/engines/${encodeURIComponent(name)}`, {method: "DELETE"}),
  matches: () => request<MatchSummary[]>("/api/matches"),
  createMatch: (config: MatchConfig) => request<{id: string}>("/api/matches", {method: "POST", body: JSON.stringify(config)}),
  match: (id: string) => request<MatchDetailResponse>(`/api/matches/${encodeURIComponent(id)}`),
  games: (id: string) => request<GameRow[]>(`/api/matches/${encodeURIComponent(id)}/games`),
  game: (id: string, index: number) => request<GameDetail>(`/api/matches/${encodeURIComponent(id)}/games/${index}`),
  stopMatch: (id: string) => request<{ok: true}>(`/api/matches/${encodeURIComponent(id)}/stop`, {method: "POST"}),
  deleteMatch: (id: string) => request<{ok: true}>(`/api/matches/${encodeURIComponent(id)}`, {method: "DELETE"}),
};

export function pgnUrl(id: string, index?: number): string {
  if (index === undefined) return `/api/matches/${encodeURIComponent(id)}/games.pgn`;
  return `/api/matches/${encodeURIComponent(id)}/games/${index}.pgn`;
}
