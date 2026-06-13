import {EventEmitter} from "node:events";
import fs from "node:fs";
import path from "node:path";
import {fileURLToPath} from "node:url";
import {spawn, spawnSync} from "node:child_process";
import {Chess} from "chess.js";
import os from "node:os";
import type {
  EngineMeta,
  EngineSpec,
  GameDetail,
  GameRow,
  MatchConfig,
  MatchResults,
  MatchSettings,
  MatchStatus,
  MatchSummary,
  PersistedStatus,
} from "./types.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export const paths = {
  matchManager: path.resolve(__dirname, ".."),
  tools: path.resolve(__dirname, "..", ".."),
  root: path.resolve(__dirname, "..", "..", ".."),
};

export const ENGINES_DIR = path.join(paths.matchManager, "engines");
export const MATCHES_DIR = path.join(paths.matchManager, "matches");
export const CUTECHESS = path.join(paths.tools, "cutechess-cli");
export const OPENINGS = path.join(paths.tools, "openings.epd");
export const KARPOV_BIN = path.join(paths.root, "target", "release", "karpov");
export const STOCKFISH = findExecutable("stockfish") ?? "/usr/games/stockfish";
export const DEFAULT_CONCURRENCY = Math.max(1, Math.min(20, os.cpus().length - 4));

const SCORE_RE = /Score of (.+?) vs (.+?): (\d+) - (\d+) - (\d+)\s+\[([0-9.]+)]\s+(\d+)/;
const FINISHED_RE = /Finished game (\d+)/;
const SPRT_RE = /SPRT: llr ([-0-9.]+)/;

fs.mkdirSync(ENGINES_DIR, {recursive: true});
fs.mkdirSync(MATCHES_DIR, {recursive: true});

function findExecutable(name: string): string | null {
  const result = spawnSync("which", [name], {encoding: "utf8"});
  return result.status === 0 ? result.stdout.trim() : null;
}

export function timestamp(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}_${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
}

export function safeName(name: string): string {
  return name.trim().replace(/[^A-Za-z0-9_.+-]/g, "_").slice(0, 64);
}

export function readJson<T>(file: string, fallback: T): T {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8")) as T;
  } catch {
    return fallback;
  }
}

export function writeJson(file: string, data: unknown): void {
  const tmp = `${file}.tmp`;
  fs.writeFileSync(tmp, `${JSON.stringify(data, null, 2)}\n`);
  fs.renameSync(tmp, file);
}

export function engineLabel(spec: EngineSpec): string {
  return spec.type === "stockfish" ? `SF@${spec.elo}` : spec.name;
}

function eloFromScore(score: number): number {
  if (score <= 0) return Number.NEGATIVE_INFINITY;
  if (score >= 1) return Number.POSITIVE_INFINITY;
  return -400.0 * Math.log10(1.0 / score - 1.0);
}

function erf(x: number): number {
  const sign = x >= 0 ? 1 : -1;
  const a1 = 0.254829592;
  const a2 = -0.284496736;
  const a3 = 1.421413741;
  const a4 = -1.453152027;
  const a5 = 1.061405429;
  const p = 0.3275911;
  const abs = Math.abs(x);
  const t = 1.0 / (1.0 + p * abs);
  const y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * Math.exp(-abs * abs);
  return sign * y;
}

export function eloStats(wins: number, draws: number, losses: number): [number | null, number | null, number | null] {
  const n = wins + draws + losses;
  if (n === 0) return [null, null, null];
  const score = (wins + 0.5 * draws) / n;
  const elo = eloFromScore(score);
  const variance = (wins * (1 - score) ** 2 + draws * (0.5 - score) ** 2 + losses * score ** 2) / n;
  const stdev = Math.sqrt(variance / n);
  const lo = Math.max(score - 1.959964 * stdev, 1e-6);
  const hi = Math.min(score + 1.959964 * stdev, 1 - 1e-6);
  const err = (eloFromScore(hi) - eloFromScore(lo)) / 2;
  const los = 0.5 * (1 + erf((wins - losses) / Math.sqrt(2.0 * Math.max(wins + losses, 1))));
  if (!Number.isFinite(elo)) return [elo > 0 ? 9999.0 : -9999.0, null, round1(los * 100)];
  return [round1(elo), err ? round1(err) : null, round1(los * 100)];
}

function round1(n: number): number {
  return Math.round(n * 10) / 10;
}

export function defaultResults(): MatchResults {
  return {
    name1: null,
    name2: null,
    wins: 0,
    losses: 0,
    draws: 0,
    games_done: 0,
    elo: null,
    elo_error: null,
    los: null,
    sprt_llr: null,
    sprt_result: null,
  };
}

export function defaultSettings(): MatchSettings {
  return {
    games: 100,
    tc: "5+0.05",
    concurrency: DEFAULT_CONCURRENCY,
    hash: 128,
    openings: true,
    draw_adjudication: true,
    resign_adjudication: true,
    sprt: false,
    sprt_elo0: 0,
    sprt_elo1: 5,
  };
}

export function listEngines(): EngineMeta[] {
  return fs.readdirSync(ENGINES_DIR, {withFileTypes: true})
    .filter((entry) => entry.isDirectory())
    .map((entry) => {
      const meta = readJson<EngineMeta | null>(path.join(ENGINES_DIR, entry.name, "meta.json"), null);
      const binary = path.join(ENGINES_DIR, entry.name, "karpov");
      return meta && fs.existsSync(binary) ? meta : null;
    })
    .filter((meta): meta is EngineMeta => Boolean(meta))
    .sort((a, b) => a.name.localeCompare(b.name));
}

export async function snapshotEngine(nameInput: string, note = ""): Promise<EngineMeta> {
  const name = safeName(nameInput);
  if (!name) throw new Error("Snapshot name is required");
  const destDir = path.join(ENGINES_DIR, name);
  if (fs.existsSync(destDir)) throw new Error(`Snapshot '${name}' already exists`);

  await runCommand("cargo", ["build", "--release"], paths.root, 300_000);
  if (!fs.existsSync(KARPOV_BIN)) throw new Error(`Build completed but binary was not found at ${KARPOV_BIN}`);

  fs.mkdirSync(destDir);
  const destBin = path.join(destDir, "karpov");
  fs.copyFileSync(KARPOV_BIN, destBin);
  fs.chmodSync(destBin, fs.statSync(destBin).mode | 0o111);
  const meta: EngineMeta = {
    name,
    note,
    created: new Date().toISOString().slice(0, 19),
    source: "cargo build --release",
  };
  writeJson(path.join(destDir, "meta.json"), meta);
  return meta;
}

export function importEngine(nameInput: string, binaryPath: string, note = ""): EngineMeta {
  const name = safeName(nameInput);
  if (!name) throw new Error("Name is required");
  const source = path.resolve(binaryPath.replace(/^~/, process.env.HOME ?? "~"));
  if (!fs.existsSync(source) || !fs.statSync(source).isFile()) throw new Error(`No such file: ${source}`);
  const destDir = path.join(ENGINES_DIR, name);
  if (fs.existsSync(destDir)) throw new Error(`Snapshot '${name}' already exists`);
  fs.mkdirSync(destDir);
  const destBin = path.join(destDir, "karpov");
  fs.copyFileSync(source, destBin);
  fs.chmodSync(destBin, fs.statSync(destBin).mode | 0o111);
  const meta: EngineMeta = {
    name,
    note,
    created: new Date().toISOString().slice(0, 19),
    source,
  };
  writeJson(path.join(destDir, "meta.json"), meta);
  return meta;
}

function runCommand(command: string, args: string[], cwd: string, timeoutMs: number): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {cwd, stdio: ["ignore", "pipe", "pipe"]});
    let stderr = "";
    const timeout = setTimeout(() => {
      child.kill("SIGTERM");
      reject(new Error(`${command} timed out`));
    }, timeoutMs);
    child.stderr.on("data", (chunk: Buffer) => {
      stderr = `${stderr}${chunk.toString()}`.slice(-3000);
    });
    child.on("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    child.on("close", (code) => {
      clearTimeout(timeout);
      if (code === 0) resolve();
      else reject(new Error(`${command} ${args.join(" ")} failed:\n${stderr}`));
    });
  });
}

function parseExtraOptions(text = ""): Record<string, string> {
  const opts: Record<string, string> = {};
  for (const part of text.split(",")) {
    const trimmed = part.trim();
    if (!trimmed.includes("=")) continue;
    const [key, ...rest] = trimmed.split("=");
    opts[key.trim()] = rest.join("=").trim();
  }
  return opts;
}

function resolveEngineSpec(spec: EngineSpec, settings: MatchSettings, usedNames: Set<string>): [string, string, Record<string, string>] {
  let cmd: string;
  let name: string;
  let opts: Record<string, string>;
  if (spec.type === "stockfish") {
    const elo = Number(spec.elo || 2000);
    name = `SF_${elo}`;
    opts = {
      UCI_LimitStrength: "true",
      UCI_Elo: String(elo),
      Threads: "1",
      Hash: "16",
    };
    cmd = STOCKFISH;
  } else {
    const snapshot = safeName(spec.name);
    cmd = path.join(ENGINES_DIR, snapshot, "karpov");
    if (!fs.existsSync(cmd)) throw new Error(`No snapshot named '${snapshot}'`);
    name = snapshot;
    opts = {Hash: String(settings.hash ?? 128)};
  }
  Object.assign(opts, parseExtraOptions(spec.extra_options));
  while (usedNames.has(name)) name = `${name}_2`;
  usedNames.add(name);
  return [cmd, name, opts];
}

export class ManagedMatch extends EventEmitter {
  readonly id: string;
  readonly config: MatchConfig;
  readonly dir: string;
  readonly pgnPath: string;
  readonly logPath: string;
  status: MatchStatus = "pending";
  error: string | null = null;
  results: MatchResults = defaultResults();
  started: string | null = null;
  finished: string | null = null;
  private process: ReturnType<typeof spawn> | null = null;

  constructor(id: string, config: MatchConfig, restore?: PersistedStatus) {
    super();
    this.id = id;
    this.config = config;
    this.dir = path.join(MATCHES_DIR, id);
    this.pgnPath = path.join(this.dir, "games.pgn");
    this.logPath = path.join(this.dir, "cutechess.log");
    if (restore) {
      this.status = restore.status ?? "interrupted";
      this.error = restore.error ?? null;
      this.results = {...this.results, ...restore.results};
      this.started = restore.started ?? null;
      this.finished = restore.finished ?? null;
      if (this.status === "running") this.status = "interrupted";
    }
  }

  buildCommand(): string[] {
    const settings = this.config.settings;
    const used = new Set<string>();
    const [cmd1, name1, opts1] = resolveEngineSpec(this.config.white, settings, used);
    const [cmd2, name2, opts2] = resolveEngineSpec(this.config.black, settings, used);
    this.results.name1 = name1;
    this.results.name2 = name2;

    const command = [CUTECHESS];
    for (const [cmd, name, opts] of [[cmd1, name1, opts1], [cmd2, name2, opts2]] as const) {
      command.push("-engine", `cmd=${cmd}`, "proto=uci", `name=${name}`);
      for (const [key, value] of Object.entries(opts)) command.push(`option.${key}=${value}`);
    }
    command.push("-each", "proto=uci", `tc=${settings.tc}`);
    const games = Math.max(2, Number(settings.games || 100));
    command.push("-games", "2", "-rounds", String(Math.ceil(games / 2)), "-repeat");
    command.push("-concurrency", String(Number(settings.concurrency || DEFAULT_CONCURRENCY)));
    if (settings.openings) command.push("-openings", `file=${OPENINGS}`, "format=epd", "order=random", "policy=round");
    command.push("-recover", "-maxmoves", "200");
    if (settings.draw_adjudication) command.push("-draw", "movenumber=40", "movecount=8", "score=10");
    if (settings.resign_adjudication) command.push("-resign", "movecount=5", "score=700", "twosided=true");
    if (settings.sprt) command.push("-sprt", `elo0=${settings.sprt_elo0}`, `elo1=${settings.sprt_elo1}`, "alpha=0.05", "beta=0.05");
    command.push("-pgnout", this.pgnPath, "-ratinginterval", "10");
    return command;
  }

  start(): void {
    fs.mkdirSync(this.dir, {recursive: true});
    writeJson(path.join(this.dir, "config.json"), this.config);
    let command: string[];
    try {
      command = this.buildCommand();
    } catch (error) {
      this.status = "error";
      this.error = error instanceof Error ? error.message : String(error);
      this.persist();
      this.emit("change");
      return;
    }
    this.status = "running";
    this.started = new Date().toISOString().slice(0, 19);
    this.persist();
    fs.writeFileSync(this.logPath, `${command.join(" ")}\n\n`);
    this.process = spawn(command[0], command.slice(1), {stdio: ["ignore", "pipe", "pipe"]});
    const handleData = (chunk: Buffer) => {
      const text = chunk.toString();
      fs.appendFileSync(this.logPath, text);
      for (const line of text.split(/\r?\n/)) this.ingest(line.trimEnd());
    };
    this.process.stdout?.on("data", handleData);
    this.process.stderr?.on("data", handleData);
    this.process.on("error", (error) => {
      this.status = "error";
      this.error = error.message;
      this.finished = new Date().toISOString().slice(0, 19);
      this.persist();
      this.emit("change");
    });
    this.process.on("close", (code) => {
      if (this.status === "running") {
        this.status = code === 0 ? "finished" : "error";
        if (this.status === "error") this.error = `cutechess exited with code ${code}`;
      }
      this.finished = new Date().toISOString().slice(0, 19);
      this.persist();
      this.emit("change");
    });
    this.emit("change");
  }

  ingest(line: string): void {
    let match = SCORE_RE.exec(line);
    if (match) {
      this.results.wins = Number(match[3]);
      this.results.losses = Number(match[4]);
      this.results.draws = Number(match[5]);
      this.results.games_done = Number(match[7]);
      const [elo, eloError, los] = eloStats(this.results.wins, this.results.draws, this.results.losses);
      this.results.elo = elo;
      this.results.elo_error = eloError;
      this.results.los = los;
    }
    match = FINISHED_RE.exec(line);
    if (match) this.results.games_done = Math.max(this.results.games_done, Number(match[1]));
    match = SPRT_RE.exec(line);
    if (match) this.results.sprt_llr = Number(match[1]);
    if (line.includes("SPRT: llr")) {
      if (line.includes("H1 was accepted")) this.results.sprt_result = "PASSED";
      else if (line.includes("H0 was accepted")) this.results.sprt_result = "FAILED";
    }
    if (this.results.games_done % 10 === 0) this.persist();
    this.emit("change");
  }

  stop(): void {
    if (this.status === "running" && this.process) {
      this.process.kill("SIGTERM");
      this.status = "stopped";
      this.finished = new Date().toISOString().slice(0, 19);
      this.persist();
      this.emit("change");
    }
  }

  persist(): void {
    writeJson(path.join(this.dir, "status.json"), {
      status: this.status,
      error: this.error,
      results: this.results,
      started: this.started,
      finished: this.finished,
    });
  }

  summary(): MatchSummary {
    return {
      id: this.id,
      white: this.config.white,
      black: this.config.black,
      settings: this.config.settings,
      status: this.status,
      error: this.error,
      started: this.started,
      finished: this.finished,
      results: {...this.results},
    };
  }

  gamesList(): GameRow[] {
    return splitPgnGames(this.pgnPath).map((text, index) => {
      const headers = parsePgnHeaders(text);
      return {
        index,
        white: headers.White ?? "?",
        black: headers.Black ?? "?",
        result: headers.Result ?? "*",
        plies: headers.PlyCount ?? "?",
        termination: headers.Termination ?? "normal",
        round: headers.Round ?? "?",
      };
    });
  }

  gameDetail(index: number): GameDetail | null {
    const gameText = splitPgnGames(this.pgnPath)[index];
    if (!gameText) return null;
    const chess = new Chess();
    try {
      chess.loadPgn(gameText, {strict: false});
    } catch {
      return null;
    }
    const headers = chess.getHeaders();
    const sans = chess.history();
    const replay = headers.FEN ? new Chess(headers.FEN) : new Chess();
    const fens = [replay.fen()];
    for (const san of sans) {
      replay.move(san);
      fens.push(replay.fen());
    }
    return {headers, sans, fens};
  }
}

export class MatchStore extends EventEmitter {
  readonly matches = new Map<string, ManagedMatch>();

  load(): void {
    this.matches.clear();
    for (const id of fs.readdirSync(MATCHES_DIR).sort()) {
      const dir = path.join(MATCHES_DIR, id);
      if (!fs.statSync(dir).isDirectory()) continue;
      const config = readJson<MatchConfig | null>(path.join(dir, "config.json"), null);
      if (!config) continue;
      const status = readJson<PersistedStatus>(path.join(dir, "status.json"), {});
      const match = new ManagedMatch(id, config, status);
      match.on("change", () => this.emit("change"));
      this.matches.set(id, match);
    }
    this.emit("change");
  }

  summaries(): MatchSummary[] {
    return [...this.matches.values()].map((match) => match.summary()).sort((a, b) => b.id.localeCompare(a.id));
  }

  get(id: string): ManagedMatch | undefined {
    return this.matches.get(id);
  }

  start(config: MatchConfig): ManagedMatch {
    let id = `m_${timestamp()}`;
    while (this.matches.has(id)) id += "x";
    const match = new ManagedMatch(id, config);
    match.on("change", () => this.emit("change"));
    this.matches.set(id, match);
    match.start();
    this.emit("change");
    return match;
  }

  deleteMatch(id: string): void {
    const match = this.matches.get(id);
    if (!match) throw new Error("No such match");
    if (match.status === "running") throw new Error("Stop the match before deleting it");
    fs.rmSync(match.dir, {recursive: true, force: true});
    this.matches.delete(id);
    this.emit("change");
  }

  deleteEngine(nameInput: string): void {
    const name = safeName(nameInput);
    for (const match of this.matches.values()) {
      if (match.status !== "running") continue;
      const white = match.config.white.type === "snapshot" ? match.config.white.name : null;
      const black = match.config.black.type === "snapshot" ? match.config.black.name : null;
      if (name === white || name === black) throw new Error(`'${name}' is used by a running match`);
    }
    const dir = path.join(ENGINES_DIR, name);
    if (!fs.existsSync(dir)) throw new Error(`No snapshot named '${name}'`);
    fs.rmSync(dir, {recursive: true, force: true});
    this.emit("change");
  }

  stopAll(): void {
    for (const match of this.matches.values()) match.stop();
  }
}

function parsePgnHeaders(text: string): Record<string, string> {
  const headers: Record<string, string> = {};
  const re = /^\[([A-Za-z0-9_]+)\s+"((?:\\"|[^"])*)"]/gm;
  let match: RegExpExecArray | null;
  while ((match = re.exec(text))) {
    headers[match[1]] = match[2].replace(/\\"/g, "\"");
  }
  return headers;
}

function splitPgnGames(file: string): string[] {
  if (!fs.existsSync(file)) return [];
  const text = fs.readFileSync(file, "utf8").trim();
  if (!text) return [];
  return text.split(/\n\s*\n(?=\[Event\s+")/).map((game) => game.trim()).filter(Boolean);
}
