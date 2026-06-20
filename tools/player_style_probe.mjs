#!/usr/bin/env node
import {spawn, spawnSync} from "node:child_process";
import fs from "node:fs";
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
  console.log(`Usage: node tools/player_style_probe.mjs [options]

Compare a UCI engine's fixed-depth move choices with a reference player's PGN
moves, then report exact move matches and immediate opponent legal replies.

Input:
  --pgn FILE             Read games from a PGN file.
  --zip FILE             Read games from a zip archive. Default: games/Karpov.zip
  --member NAME          PGN member inside the zip. Default: first .pgn member
  --player REGEX         Reference player name regex. Default: karpov
  --label TEXT           Display label for the player. Default: --player

Engine and sampling:
  --engine FILE          UCI engine path. Default: target/release/boa
  --depth N              Fixed search depth. Default: 5
  --positions N          Maximum sampled positions. Default: 200
  --stride N             Keep every Nth eligible move. Default: 17
  --min-ply N            First eligible ply. Default: 16
  --max-ply N            Last eligible ply. Default: 90
  --samples N            Disagreement samples to print. Default: 12
  --progress N           Progress interval. Default: 25

Examples:
  node tools/player_style_probe.mjs --depth 4 --positions 80
  node tools/player_style_probe.mjs --zip games/Petrosian.zip --player petrosian
  node tools/player_style_probe.mjs --pgn games.pgn --player "karpov|petrosian" --label karpov_petrossian
`);
  process.exit(0);
}

const zipPath = args.get("zip") ?? "games/Karpov.zip";
const pgnPath = args.get("pgn") ?? null;
let member = args.get("member") ?? null;
const enginePath = args.get("engine") ?? "target/release/boa";
const playerName = args.get("player") ?? "karpov";
const playerLabel = args.get("label") ?? playerName;
const playerPattern = regexArg(playerName);
const depth = intArg("depth", 5, 1);
const maxPositions = intArg("positions", 200, 1);
const stride = intArg("stride", 17, 1);
const minPly = intArg("min-ply", 16, 0);
const maxPly = intArg("max-ply", 90, 0);
const sampleLimit = intArg("samples", 12, 0);
const progressEvery = intArg("progress", 25, 0);
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

function regexArg(pattern) {
  try {
    return new RegExp(pattern, "i");
  } catch (error) {
    fail(`Invalid --player regex '${pattern}': ${error.message}`);
  }
}

function readPgnInput() {
  if (pgnPath) {
    try {
      return {
        source: pgnPath,
        text: fs.readFileSync(pgnPath, "utf8"),
      };
    } catch (error) {
      fail(`Could not read ${pgnPath}: ${error.message}`);
    }
  }

  if (!member) {
    const listing = spawnSync("unzip", ["-Z1", zipPath], {
      encoding: "utf8",
      maxBuffer: 16 * 1024 * 1024,
    });
    if (listing.status !== 0) fail(`Could not list ${zipPath}:\n${listing.stderr}`);
    member = listing.stdout
      .split(/\r?\n/)
      .map((line) => line.trim())
      .find((line) => /\.pgn$/i.test(line));
    if (!member) fail(`No .pgn member found in ${zipPath}`);
  }

  const unzip = spawnSync("unzip", ["-p", zipPath, member], {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (unzip.status !== 0) fail(`Could not read ${member} from ${zipPath}:\n${unzip.stderr}`);
  return {
    source: `${zipPath}:${member}`,
    text: unzip.stdout,
  };
}

function splitPgnGames(text) {
  const chunks = text.replace(/\r\n/g, "\n").split(/\n(?=\[Event\s+")/g);
  return chunks.map((chunk) => chunk.trim()).filter(Boolean);
}

function playerSide(headers) {
  const white = headers.White ?? "";
  const black = headers.Black ?? "";
  if (playerPattern.test(white)) return "w";
  if (playerPattern.test(black)) return "b";
  return null;
}

function moveToUci(move) {
  return `${move.from}${move.to}${move.promotion ?? ""}`;
}

function mobilityAfter(fen, uci) {
  const board = new Chess(fen);
  const moves = board.moves({verbose: true});
  const move = moves.find((candidate) => moveToUci(candidate) === uci);
  if (!move) return null;
  board.move(move);
  return board.moves().length;
}

function collectPositions(games) {
  const positions = [];
  let eligible = 0;

  for (let gameIndex = 0; gameIndex < games.length; gameIndex++) {
    const game = new Chess();
    try {
      game.loadPgn(games[gameIndex], {strict: false});
    } catch {
      continue;
    }

    const side = playerSide(game.getHeaders());
    if (!side) continue;

    const headers = game.getHeaders();
    for (const move of game.history({verbose: true})) {
      const ply = Number(move.after.split(" ")[5]) * 2 - (move.color === "w" ? 1 : 0);
      if (move.color !== side || ply < minPly || ply > maxPly) continue;

      eligible++;
      if (eligible % stride !== 0) continue;

      positions.push({
        gameIndex: gameIndex + 1,
        event: headers.Event ?? "?",
        date: headers.Date ?? "?",
        white: headers.White ?? "?",
        black: headers.Black ?? "?",
        ply,
        fen: move.before,
        referenceSan: move.san,
        referenceUci: moveToUci(move),
      });

      if (positions.length >= maxPositions) return positions;
    }
  }

  return positions;
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

  async bestMove(fen) {
    this.send(`position fen ${fen}`);
    this.send(`go depth ${depth}`);
    const line = await this.waitFor((candidate) => candidate.startsWith("bestmove "));
    return line.split(/\s+/)[1];
  }

  quit() {
    this.send("quit");
  }
}

const input = readPgnInput();
const games = splitPgnGames(input.text);
const positions = collectPositions(games);
if (positions.length === 0) fail(`No positions found for player pattern ${playerPattern}.`);

const engine = new UciEngine(enginePath);
await engine.init();

let exact = 0;
let boaMoreRestrictive = 0;
let referenceMoreRestrictive = 0;
let sameRestriction = 0;
let comparedRestriction = 0;
const samples = [];

for (let i = 0; i < positions.length; i++) {
  const pos = positions[i];
  const boaUci = await engine.bestMove(pos.fen);
  const boaMobility = mobilityAfter(pos.fen, boaUci);
  const referenceMobility = mobilityAfter(pos.fen, pos.referenceUci);

  if (boaUci === pos.referenceUci) exact++;
  if (boaMobility !== null && referenceMobility !== null) {
    comparedRestriction++;
    if (boaMobility < referenceMobility) boaMoreRestrictive++;
    else if (boaMobility > referenceMobility) referenceMoreRestrictive++;
    else sameRestriction++;
  }

  if (samples.length < sampleLimit && boaUci !== pos.referenceUci) {
    samples.push({...pos, boaUci, boaMobility, referenceMobility});
  }

  if (progressEvery > 0 && (i + 1) % progressEvery === 0) {
    process.stderr.write(`checked ${i + 1}/${positions.length}\n`);
  }
}

engine.quit();

const pct = (n, d) => `${((100 * n) / Math.max(1, d)).toFixed(1)}%`;
console.log(`# Player Style Probe`);
console.log(`source: ${input.source}`);
console.log(`games: ${games.length}`);
console.log(`player: ${playerPattern}`);
console.log(`positions: ${positions.length}`);
console.log(`depth: ${depth}`);
console.log(`ply window: ${minPly}-${maxPly}`);
console.log(`stride: ${stride}`);
console.log("");
console.log(`exact ${playerLabel} move matches: ${exact}/${positions.length} (${pct(exact, positions.length)})`);
console.log(`Boa leaves fewer opponent legal replies: ${boaMoreRestrictive}/${comparedRestriction} (${pct(boaMoreRestrictive, comparedRestriction)})`);
console.log(`${playerLabel} leaves fewer opponent legal replies: ${referenceMoreRestrictive}/${comparedRestriction} (${pct(referenceMoreRestrictive, comparedRestriction)})`);
console.log(`same opponent legal replies: ${sameRestriction}/${comparedRestriction} (${pct(sameRestriction, comparedRestriction)})`);
console.log("");
console.log("sample disagreements:");
for (const sample of samples) {
  console.log(
    `- game ${sample.gameIndex}, ply ${sample.ply}, ${sample.white} vs ${sample.black}: ${playerLabel} ${sample.referenceSan} (${sample.referenceUci}, opp replies ${sample.referenceMobility}) vs Boa ${sample.boaUci} (opp replies ${sample.boaMobility})`,
  );
}
