#!/usr/bin/env node
import {spawnSync} from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

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
  console.log(`Usage: node tools/self_play_dataset.mjs [options]

Generate a Boa self-play PGN with cutechess, then extract the same diagnostic
CSV rows used by tools/texel_tune.py.

Options:
  --engine FILE          Boa binary. Default: target/release/boa
  --cutechess FILE       cutechess-cli binary. Default: tools/cutechess-cli
  --openings FILE        Opening suite. Default: tools/openings.epd
  --pgn FILE             Self-play PGN output. Default: analysis/self_play/self_play.pgn
  --out FILE             Feature CSV output. Default: analysis/self_play/texel_self_play.csv
  --games N             Scored games to request. Default: 200
  --tc VALUE             cutechess time control. Default: 1+0.01
  --concurrency N        Parallel games. Default: available CPUs capped at 12
  --positions N          Maximum extracted rows. Default: 500000
  --stride N             Keep every Nth eligible position. Default: 1
  --min-ply N            First eligible ply. Default: 12
  --max-ply N            Last eligible ply. Default: 100
  --future-plies N       Static eval label this many plies later. Default: 4
  --quiet                Keep only quiet positions during extraction
  --skip-games           Reuse an existing --pgn and only extract CSV rows
  --progress N           Extraction progress interval. Default: 25000

Example:
  cargo build --release
  node tools/self_play_dataset.mjs --games 1000 --quiet
`);
  process.exit(0);
}

const enginePath = args.get("engine") ?? "target/release/boa";
const cutechessPath = args.get("cutechess") ?? "tools/cutechess-cli";
const openingsPath = args.get("openings") ?? "tools/openings.epd";
const pgnPath = args.get("pgn") ?? "analysis/self_play/self_play.pgn";
const outPath = args.get("out") ?? "analysis/self_play/texel_self_play.csv";
const games = intArg("games", 200, 2);
const tc = args.get("tc") ?? "1+0.01";
const concurrency = intArg("concurrency", Math.min(os.availableParallelism?.() ?? os.cpus().length, 12), 1);
const positions = intArg("positions", 500000, 1);
const stride = intArg("stride", 1, 1);
const minPly = intArg("min-ply", 12, 0);
const maxPly = intArg("max-ply", 100, 0);
const futurePlies = intArg("future-plies", 4, 1);
const progressEvery = intArg("progress", 25000, 0);
const quiet = args.has("quiet");
const skipGames = args.has("skip-games");

if (minPly > maxPly) fail("--min-ply cannot be greater than --max-ply");
if (games % 2 !== 0) fail("--games must be even so cutechess can alternate colors cleanly");

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

function run(command, commandArgs) {
  console.error([command, ...commandArgs].join(" "));
  const result = spawnSync(command, commandArgs, {stdio: "inherit"});
  if (result.error) fail(result.error.message);
  if (result.status !== 0) fail(`${command} exited with status ${result.status}`);
}

fs.mkdirSync(path.dirname(pgnPath), {recursive: true});
fs.mkdirSync(path.dirname(outPath), {recursive: true});

if (!skipGames) {
  const rounds = Math.ceil(games / 2);
  run(cutechessPath, [
    "-engine",
    `cmd=${enginePath}`,
    "proto=uci",
    "name=boa-a",
    "option.Hash=64",
    "-engine",
    `cmd=${enginePath}`,
    "proto=uci",
    "name=boa-b",
    "option.Hash=64",
    "-each",
    "proto=uci",
    `tc=${tc}`,
    "-games",
    "2",
    "-rounds",
    String(rounds),
    "-repeat",
    "-concurrency",
    String(concurrency),
    "-openings",
    `file=${openingsPath}`,
    "format=epd",
    "order=random",
    "policy=round",
    "-recover",
    "-maxmoves",
    "200",
    "-draw",
    "movenumber=40",
    "movecount=8",
    "score=10",
    "-resign",
    "movecount=5",
    "score=700",
    "twosided=true",
    "-pgnout",
    pgnPath,
  ]);
}

const extractorArgs = [
  "tools/restriction_signal.mjs",
  "--engine",
  enginePath,
  "--pgn",
  pgnPath,
  "--out",
  outPath,
  "--positions",
  String(positions),
  "--stride",
  String(stride),
  "--min-ply",
  String(minPly),
  "--max-ply",
  String(maxPly),
  "--future-plies",
  String(futurePlies),
  "--progress",
  String(progressEvery),
];
if (quiet) extractorArgs.push("--quiet");

run(process.execPath, extractorArgs);
console.log(`self-play dataset written to ${outPath}`);
