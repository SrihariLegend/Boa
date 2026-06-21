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
  console.log(`Usage: node tools/criticality_dataset.mjs [options]

Generate Boa self-play games while logging learned-move-criticality samples.

Options:
  --engine FILE          Boa binary. Default: target/release/boa
  --cutechess FILE       cutechess-cli binary. Default: tools/cutechess-cli
  --openings FILE        Opening suite. Default: tools/openings.epd
  --run-dir DIR          Output run directory. Default: analysis/criticality/<timestamp>
  --games N             Scored games to request. Default: 1000
  --tc VALUE             cutechess time control. Default: 1+0.01
  --concurrency N        Parallel games. Default: available CPUs capped at 12
  --probe-permille N     Counterfactual probe rate. Default: 5

Example:
  cargo build --release
  node tools/criticality_dataset.mjs --games 200 --probe-permille 5
`);
  process.exit(0);
}

const enginePath = path.resolve(args.get("engine") ?? "target/release/boa");
const cutechessPath = args.get("cutechess") ?? "tools/cutechess-cli";
const openingsPath = args.get("openings") ?? "tools/openings.epd";
const runDir = args.get("run-dir") ?? path.join("analysis", "criticality", timestamp());
const rawDir = path.resolve(path.join(runDir, "raw"));
const pgnPath = path.join(runDir, "games.pgn");
const logPath = path.join(runDir, "cutechess.log");
const games = intArg("games", 1000, 2);
const tc = args.get("tc") ?? "1+0.01";
const concurrency = intArg("concurrency", Math.min(os.availableParallelism?.() ?? os.cpus().length, 12), 1);
const probePermille = intArg("probe-permille", 5, 0, 1000);

if (games % 2 !== 0) fail("--games must be even so cutechess can alternate colors cleanly");

fs.mkdirSync(rawDir, {recursive: true});
fs.mkdirSync(runDir, {recursive: true});
const wrapperPath = path.join(runDir, "boa-criticality-wrapper.sh");
fs.writeFileSync(
  wrapperPath,
  `#!/usr/bin/env sh\nexport BOA_CRITICALITY_LOG_DIR='${shellQuoteValue(rawDir)}'\nexport BOA_CRITICALITY_PROBE_PERMILLE='${probePermille}'\nexec '${shellQuoteValue(enginePath)}' \"$@\"\n`,
  {mode: 0o755},
);
const rounds = Math.ceil(games / 2);
const commandArgs = [
  "-engine",
  `cmd=${wrapperPath}`,
  "proto=uci",
  "name=boa-a",
  "option.Hash=64",
  "-engine",
  `cmd=${wrapperPath}`,
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
];

console.error([cutechessPath, ...commandArgs].join(" "));
const log = fs.openSync(logPath, "w");
const result = spawnSync(cutechessPath, commandArgs, {
  stdio: ["ignore", "pipe", "pipe"],
  encoding: "utf8",
  maxBuffer: 100 * 1024 * 1024,
});
fs.writeFileSync(log, result.stdout ?? "");
fs.writeSync(log, result.stderr ?? "");
fs.closeSync(log);
process.stdout.write(result.stdout ?? "");
process.stderr.write(result.stderr ?? "");
if (result.error) fail(result.error.message);
if (result.status !== 0) fail(`${cutechessPath} exited with status ${result.status}`);

console.log(`criticality raw CSV written to ${rawDir}`);
console.log(`PGN written to ${pgnPath}`);

function fail(message) {
  console.error(message);
  process.exit(1);
}

function intArg(name, fallback, min, max = Number.MAX_SAFE_INTEGER) {
  const raw = args.get(name);
  if (raw === undefined) return fallback;
  const value = Number(raw);
  if (!Number.isInteger(value) || value < min || value > max) {
    fail(`--${name} must be an integer in [${min}, ${max}]`);
  }
  return value;
}

function timestamp() {
  return new Date().toISOString().replaceAll(":", "").replaceAll(".", "").replace("T", "_").replace("Z", "");
}

function shellQuoteValue(value) {
  return String(value).replaceAll("'", "'\"'\"'");
}
