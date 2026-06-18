#!/usr/bin/env node
import {spawn, spawnSync} from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import {Chess} from "./match_manager/node_modules/chess.js/dist/esm/chess.js";

const args = new Map();
for (let i = 2; i < process.argv.length; i++) {
  const arg = process.argv[i];
  if (!arg.startsWith("--")) continue;
  const key = arg.slice(2);
  const next = process.argv[i + 1];
  if (next && !next.startsWith("--")) {
    args.set(key, next);
    i++;
  } else {
    args.set(key, "true");
  }
}

if (args.has("help") || args.has("h")) {
  console.log(`Usage: node tools/restriction_signal.mjs [options]

Extract Boa restriction-signal rows from PGN archives. The script uses chess.js
for PGN/FEN traversal and Boa's diagnostic UCI commands for feature extraction.

Options:
  --engine FILE          Boa binary. Default: target/release/boa
  --archives LIST        Comma-separated zip archives. Default: games/Karpov.zip,games/Petrosian.zip,games/Keres.zip
  --pgn FILE             Read one plain PGN file instead of archives
  --out FILE             CSV output. Default: analysis/restriction_signal/gm_features.csv
  --positions N          Maximum rows. Default: 500000
  --stride N             Keep every Nth eligible position. Default: 1
  --min-ply N            First eligible ply. Default: 12
  --max-ply N            Last eligible ply. Default: 100
  --future-plies N       Static eval label this many plies later. Default: 4
  --quiet                Keep only quiet positions: not check, next move not capture or promotion
  --progress N           Progress interval. Default: 1000

Example:
  cargo build --release
  node tools/restriction_signal.mjs --positions 10000 --stride 3
`);
  process.exit(0);
}

const enginePath = args.get("engine") ?? "target/release/boa";
const archivePaths = (args.get("archives") ?? "games/Karpov.zip,games/Petrosian.zip,games/Keres.zip")
  .split(",")
  .map((item) => item.trim())
  .filter(Boolean);
const pgnPath = args.get("pgn") ?? null;
const outPath = args.get("out") ?? "analysis/restriction_signal/gm_features.csv";
const maxPositions = intArg("positions", 500000, 1);
const stride = intArg("stride", 1, 1);
const minPly = intArg("min-ply", 12, 0);
const maxPly = intArg("max-ply", 100, 0);
const futurePlies = intArg("future-plies", 4, 1);
const quietOnly = args.has("quiet");
const progressEvery = intArg("progress", 1000, 0);

if (minPly > maxPly) fail("--min-ply cannot be greater than --max-ply");

function fail(message) {
  console.error(message);
  process.exit(1);
}

function intArg(name, fallback, min) {
  const raw = args.get(name);
  if (raw === undefined) return fallback;
  const value = Number(raw);
  if (!Number.isInteger(value) || value < min) {
    fail(`--${name} must be an integer >= ${min}`);
  }
  return value;
}

function csv(value) {
  const text = String(value ?? "");
  return `"${text.replaceAll("\"", "\"\"")}"`;
}

function parseCsvLine(line) {
  const fields = [];
  let current = "";
  let quoted = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (quoted) {
      if (ch === "\"" && line[i + 1] === "\"") {
        current += "\"";
        i++;
      } else if (ch === "\"") {
        quoted = false;
      } else {
        current += ch;
      }
      continue;
    }
    if (ch === "\"") {
      quoted = true;
    } else if (ch === ",") {
      fields.push(current);
      current = "";
    } else {
      current += ch;
    }
  }
  fields.push(current);
  return fields;
}

function readInputs() {
  if (pgnPath) {
    return [{source: pgnPath, text: fs.readFileSync(pgnPath, "utf8")}];
  }
  return archivePaths.map(readZipPgn);
}

function readZipPgn(zipPath) {
  const listing = spawnSync("unzip", ["-Z1", zipPath], {
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
  });
  if (listing.status !== 0) fail(`Could not list ${zipPath}:\n${listing.stderr}`);

  const member = listing.stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => /\.pgn$/i.test(line));
  if (!member) fail(`No .pgn member found in ${zipPath}`);

  const unzip = spawnSync("unzip", ["-p", zipPath, member], {
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
  });
  if (unzip.status !== 0) fail(`Could not read ${member} from ${zipPath}:\n${unzip.stderr}`);
  return {source: `${zipPath}:${member}`, text: unzip.stdout};
}

function splitPgnGames(text) {
  return text
    .replace(/\r\n/g, "\n")
    .split(/\n(?=\[Event\s+")/g)
    .map((chunk) => chunk.trim())
    .filter(Boolean);
}

function resultScore(result) {
  if (result === "1-0") return 1;
  if (result === "0-1") return -1;
  if (result === "1/2-1/2") return 0;
  return "";
}

function isQuietPosition(game, move) {
  if (game.inCheck()) return false;
  return !move.captured && !move.promotion;
}

function replayMove(game, move) {
  const replayed = game.move({
    from: move.from,
    to: move.to,
    promotion: move.promotion,
  });
  if (!replayed) {
    throw new Error(`Could not replay move ${move.lan ?? move.san ?? `${move.from}${move.to}`}`);
  }
}

class UciEngine {
  constructor(command) {
    this.child = spawn(command, [], {stdio: ["pipe", "pipe", "inherit"]});
    this.lines = [];
    this.waiters = [];
    this.child.stdout.setEncoding("utf8");
    this.child.stdout.on("data", (chunk) => {
      for (const line of chunk.split(/\r?\n/)) {
        if (!line) continue;
        const waiter = this.waiters[0];
        if (waiter && waiter.predicate(line)) {
          this.waiters.shift();
          waiter.resolve(line);
        } else {
          this.lines.push(line);
        }
      }
    });
  }

  send(command) {
    this.child.stdin.write(`${command}\n`);
  }

  waitFor(predicate) {
    const index = this.lines.findIndex(predicate);
    if (index >= 0) {
      const [line] = this.lines.splice(index, 1);
      return Promise.resolve(line);
    }
    return new Promise((resolve) => this.waiters.push({predicate, resolve}));
  }

  async init() {
    this.send("uci");
    await this.waitFor((line) => line === "uciok");
    this.send("isready");
    await this.waitFor((line) => line === "readyok");
  }

  async featureHeader() {
    this.send("restriction_features_header");
    return this.waitFor((line) => line.startsWith("fen,side_to_move,"));
  }

  async features(fen) {
    this.send(`position fen ${fen}`);
    this.send("restriction_features");
    return this.waitFor((line) => line.startsWith("\""));
  }

  quit() {
    this.send("quit");
  }
}

function collectCandidates(inputs) {
  const candidates = [];
  let eligible = 0;

  for (const input of inputs) {
    const games = splitPgnGames(input.text);
    for (let gameIndex = 0; gameIndex < games.length; gameIndex++) {
      const game = new Chess();
      try {
        game.loadPgn(games[gameIndex], {strict: false});
      } catch {
        continue;
      }

      const headers = game.getHeaders();
      const history = game.history({verbose: true});
      const replay = headers.FEN ? new Chess(headers.FEN) : new Chess();

      for (let moveIndex = 0; moveIndex < history.length; moveIndex++) {
        const move = history[moveIndex];
        const ply = moveIndex + 1;
        const future = history[moveIndex + futurePlies - 1];
        if (!future || ply < minPly || ply > maxPly) {
          replayMove(replay, move);
          continue;
        }
        if (quietOnly && !isQuietPosition(replay, move)) {
          replayMove(replay, move);
          continue;
        }

        eligible++;
        if (eligible % stride === 0) {
          candidates.push({
            source: input.source,
            gameIndex: gameIndex + 1,
            ply,
            moveNumber: Math.ceil(ply / 2),
            result: headers.Result ?? "",
            resultScore: resultScore(headers.Result),
            white: headers.White ?? "",
            black: headers.Black ?? "",
            event: headers.Event ?? "",
            date: headers.Date ?? "",
            fen: move.before,
            futureFen: future.after,
          });
          if (candidates.length >= maxPositions) return candidates;
        }

        replayMove(replay, move);
      }
    }
  }

  return candidates;
}

const inputs = readInputs();
const candidates = collectCandidates(inputs);
if (candidates.length === 0) fail("No eligible positions found.");

fs.mkdirSync(path.dirname(outPath), {recursive: true});
const engine = new UciEngine(enginePath);
await engine.init();
const engineHeader = await engine.featureHeader();
const engineColumns = engineHeader.split(",");
const staticEvalIndex = engineColumns.indexOf("static_eval_cp");
const whiteScoreIndex = engineColumns.indexOf("white_score_cp");
if (staticEvalIndex < 0 || whiteScoreIndex < 0) fail("Engine feature header is missing eval columns.");

const out = fs.createWriteStream(outPath, {encoding: "utf8"});
out.write(
  [
    "source",
    "game_index",
    "ply",
    "move_number",
    "result",
    "result_score",
    "white",
    "black",
    "event",
    "date",
    "future_eval_cp",
    "future_white_score_cp",
    engineHeader,
  ].join(",") + "\n",
);

const featureCache = new Map();
async function cachedFeatures(fen) {
  const cached = featureCache.get(fen);
  if (cached) return cached;
  const row = await engine.features(fen);
  const parsed = parseCsvLine(row);
  const result = {
    row,
    staticEval: parsed[staticEvalIndex],
    whiteScore: parsed[whiteScoreIndex],
  };
  featureCache.set(fen, result);
  return result;
}

for (let i = 0; i < candidates.length; i++) {
  const pos = candidates[i];
  const current = await cachedFeatures(pos.fen);
  const future = await cachedFeatures(pos.futureFen);
  out.write(
    [
      csv(pos.source),
      pos.gameIndex,
      pos.ply,
      pos.moveNumber,
      csv(pos.result),
      pos.resultScore,
      csv(pos.white),
      csv(pos.black),
      csv(pos.event),
      csv(pos.date),
      future.staticEval,
      future.whiteScore,
      current.row,
    ].join(",") + "\n",
  );

  if (progressEvery > 0 && (i + 1) % progressEvery === 0) {
    process.stderr.write(`wrote ${i + 1}/${candidates.length}\n`);
  }
}

await new Promise((resolve) => out.end(resolve));
engine.quit();

console.log(`wrote ${candidates.length} rows to ${outPath}`);
