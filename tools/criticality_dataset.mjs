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

Generate Boa self-play games while logging unified criticality probe samples.

Options:
  --engine FILE          Boa binary. Default: target/release/boa
  --cutechess FILE       cutechess-cli binary. Default: tools/cutechess-cli
  --openings FILE        Opening suite. Default: tools/openings.epd
  --run-dir DIR          Output run directory. Default: analysis/criticality/<timestamp>
  --games N             Scored games to request. Default: 1000
  --tc VALUE             cutechess time control. Default: 1+0.01
  --concurrency N        Parallel games. Default: available CPUs capped at 12
  --probe-permille N     Legacy/all probe rate. Default: 5
  --lmr-probe-permille N LMR probe rate. Default: --probe-permille
  --futility-probe-permille N
                         Forward-futility probe rate. Default: --probe-permille
  --futility-borderline-probe-permille N
                         FFP probe rate when slack <= threshold. Default: --futility-probe-permille
  --futility-borderline-threshold-cp N
                         Borderline FFP slack in cp. Default: 30
  --rfp-probe-permille N RFP probe rate. Default: --probe-permille
  --rfp-borderline-probe-permille N
                         RFP probe rate when slack <= threshold. Default: --rfp-probe-permille
  --rfp-borderline-threshold-cp N
                         Borderline RFP slack in cp. Default: 40
  --probe-max-rows N     Per-engine row cap. Default: 1000000
  --parquet DIR          Write directly to this Parquet dataset dir via per-engine FIFOs
  --csv-shard-mib N      Rotate raw CSV chunks at this size when not using --parquet. Default: 64

Example:
  cargo build --release
  node tools/criticality_dataset.mjs --games 200 --probe-permille 5
  node tools/criticality_dataset.mjs --games 200 --probe-permille 0 --futility-probe-permille 1000
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
const lmrProbePermille = intArg("lmr-probe-permille", probePermille, 0, 1000);
const futilityProbePermille = intArg("futility-probe-permille", probePermille, 0, 1000);
const futilityBorderlineProbePermille = intArg("futility-borderline-probe-permille", futilityProbePermille, 0, 1000);
const futilityBorderlineThresholdCp = intArg("futility-borderline-threshold-cp", 30, 0, 500);
const rfpProbePermille = intArg("rfp-probe-permille", probePermille, 0, 1000);
const rfpBorderlineProbePermille = intArg("rfp-borderline-probe-permille", rfpProbePermille, 0, 1000);
const rfpBorderlineThresholdCp = intArg("rfp-borderline-threshold-cp", 40, 0, 500);
const probeMaxRows = intArg("probe-max-rows", 1_000_000, 0, 100_000_000);
const parquetPath = args.has("parquet") ? path.resolve(args.get("parquet")) : null;
const csvShardMib = intArg("csv-shard-mib", 64, 0);

if (games % 2 !== 0) fail("--games must be even so cutechess can alternate colors cleanly");

fs.mkdirSync(rawDir, {recursive: true});
fs.mkdirSync(runDir, {recursive: true});
const wrapperPath = path.join(runDir, "boa-criticality-wrapper.sh");
if (parquetPath) {
  fs.mkdirSync(parquetPath, {recursive: true});
  const converterPath = path.resolve("tools/criticality_to_parquet.py");
  fs.writeFileSync(
    wrapperPath,
    `#!/usr/bin/env bash
set -euo pipefail
export BOA_CRITICALITY_LOG_DIR='${shellQuoteValue(rawDir)}'
export BOA_LMR_PROBE_PERMILLE='${lmrProbePermille}'
export BOA_FUTILITY_PROBE_PERMILLE='${futilityProbePermille}'
export BOA_FUTILITY_BORDERLINE_PROBE_PERMILLE='${futilityBorderlineProbePermille}'
export BOA_FUTILITY_BORDERLINE_THRESHOLD_CP='${futilityBorderlineThresholdCp}'
export BOA_RFP_PROBE_PERMILLE='${rfpProbePermille}'
export BOA_RFP_BORDERLINE_PROBE_PERMILLE='${rfpBorderlineProbePermille}'
export BOA_RFP_BORDERLINE_THRESHOLD_CP='${rfpBorderlineThresholdCp}'
export BOA_CRITICALITY_MAX_ROWS='${probeMaxRows}'
export BOA_CRITICALITY_COMPRESS='0'
pipe_dir="$BOA_CRITICALITY_LOG_DIR/pipes"
mkdir -p "$pipe_dir" '${shellQuoteValue(parquetPath)}'
fifo="$pipe_dir/criticality-$$.fifo"
rm -f "$fifo"
mkfifo "$fifo"
python3 '${shellQuoteValue(converterPath)}' "$fifo" '${shellQuoteValue(parquetPath)}' --stream --part-prefix "$(date +%s)-$$-" &
converter=$!
cleanup() { rm -f "$fifo"; }
trap cleanup EXIT
export BOA_CRITICALITY_LOG_FILE="$fifo"
'${shellQuoteValue(enginePath)}' "$@"
status=$?
: > "$fifo" 2>/dev/null || true
wait "$converter" || status=$?
exit "$status"
`,
    {mode: 0o755},
  );
} else {
  fs.writeFileSync(
    wrapperPath,
    `#!/usr/bin/env sh\nexport BOA_CRITICALITY_LOG_DIR='${shellQuoteValue(rawDir)}'\nexport BOA_LMR_PROBE_PERMILLE='${lmrProbePermille}'\nexport BOA_FUTILITY_PROBE_PERMILLE='${futilityProbePermille}'\nexport BOA_FUTILITY_BORDERLINE_PROBE_PERMILLE='${futilityBorderlineProbePermille}'\nexport BOA_FUTILITY_BORDERLINE_THRESHOLD_CP='${futilityBorderlineThresholdCp}'\nexport BOA_RFP_PROBE_PERMILLE='${rfpProbePermille}'\nexport BOA_RFP_BORDERLINE_PROBE_PERMILLE='${rfpBorderlineProbePermille}'\nexport BOA_RFP_BORDERLINE_THRESHOLD_CP='${rfpBorderlineThresholdCp}'\nexport BOA_CRITICALITY_MAX_ROWS='${probeMaxRows}'\nexport BOA_CRITICALITY_MAX_CSV_BYTES='${csvShardMib * 1024 * 1024}'\nexport BOA_CRITICALITY_COMPRESS='1'\nexec '${shellQuoteValue(enginePath)}' \"$@\"\n`,
    {mode: 0o755},
  );
}
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

if (!parquetPath) console.log(`criticality raw CSV written to ${rawDir}`);
if (parquetPath) console.log(`criticality Parquet dataset written to ${parquetPath}`);
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
