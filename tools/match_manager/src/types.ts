export type EngineMeta = {
  name: string;
  note?: string;
  created?: string;
  source?: string;
};

export type EngineSpec =
  | { type: "snapshot"; name: string; extra_options?: string }
  | { type: "stockfish"; elo: number; extra_options?: string };

export type MatchSettings = {
  games: number;
  tc: string;
  concurrency: number;
  hash: number;
  openings: boolean;
  draw_adjudication: boolean;
  resign_adjudication: boolean;
  sprt: boolean;
  sprt_elo0: number;
  sprt_elo1: number;
};

export type MatchConfig = {
  white: EngineSpec;
  black: EngineSpec;
  settings: MatchSettings;
};

export type MatchResults = {
  name1: string | null;
  name2: string | null;
  wins: number;
  losses: number;
  draws: number;
  games_done: number;
  elo: number | null;
  elo_error: number | null;
  los: number | null;
  sprt_llr: number | null;
  sprt_result: "PASSED" | "FAILED" | null;
};

export type MatchStatus = "pending" | "running" | "finished" | "stopped" | "interrupted" | "error";

export type PersistedStatus = {
  status?: MatchStatus;
  error?: string | null;
  results?: Partial<MatchResults>;
  started?: string | null;
  finished?: string | null;
};

export type MatchSummary = {
  id: string;
  white: EngineSpec;
  black: EngineSpec;
  settings: MatchSettings;
  status: MatchStatus;
  error: string | null;
  started: string | null;
  finished: string | null;
  results: MatchResults;
};

export type GameRow = {
  index: number;
  white: string;
  black: string;
  result: string;
  plies: string;
  termination: string;
  round: string;
};

export type GameDetail = {
  headers: Record<string, string>;
  sans: string[];
  fens: string[];
};
