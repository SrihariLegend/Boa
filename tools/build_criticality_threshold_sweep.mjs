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
  console.log(`Usage: node tools/build_criticality_threshold_sweep.mjs [options]

Build release Boa binaries for a learned-LMR criticality threshold sweep.
This tool only patches/builds/copies binaries; it does not run cutechess.

Options:
  --model FILE       Training JSON. Default: analysis/criticality/2026-06-22_023112418/model-shadow-only.json
  --source FILE      Rust source to patch. Default: src/search/constants.rs
  --out-dir DIR      Output directory. Default: analysis/criticality_threshold_sweep/<timestamp>
  --percentiles CSV  Percentiles to build. Default: 90,95,97,98,99,99.5
  --dry-run          Print manifest without patching/building

Example:
  node tools/build_criticality_threshold_sweep.mjs
`);
  process.exit(0);
}

const modelPath = path.resolve(args.get("model") ?? "analysis/criticality/2026-06-22_023112418/model-shadow-only.json");
const sourcePath = path.resolve(args.get("source") ?? "src/search/constants.rs");
const outDir = path.resolve(args.get("out-dir") ?? path.join("analysis", "criticality_threshold_sweep", timestamp()));
const percentiles = (args.get("percentiles") ?? "90,95,97,98,99,99.5")
  .split(",")
  .map((value) => value.trim())
  .filter(Boolean);
const dryRun = args.has("dry-run");

const model = JSON.parse(fs.readFileSync(modelPath, "utf8"));
const thresholds = model?.full?.criticality_thresholds;
if (!thresholds || typeof thresholds !== "object") {
  fail(`missing full.criticality_thresholds in ${modelPath}`);
}

const variants = percentiles.map((percentile) => {
  const key = `validation_${percentileKey(percentile)}`;
  const value = thresholds[key];
  if (typeof value !== "number") fail(`missing numeric threshold ${key} in ${modelPath}`);
  const tag = `p${percentile.replace(".", "_")}`;
  return {
    percentile,
    key,
    tag,
    threshold: value,
    binary: path.join(outDir, `boa-criticality-shadow-${tag}`),
  };
});

const manifest = {
  created_at: new Date().toISOString(),
  host: os.hostname(),
  model: modelPath,
  source: sourcePath,
  variants,
};

if (dryRun) {
  console.log(JSON.stringify(manifest, null, 2));
  process.exit(0);
}

fs.mkdirSync(outDir, {recursive: true});
const originalSource = fs.readFileSync(sourcePath, "utf8");
const constRe = /const (CRITICALITY_(?:PROTECTION|P[0-9_]+)_LOGIT): f64 = [-+0-9.eE_]+;/;
const constMatch = originalSource.match(constRe);
if (!constMatch) {
  fail(`could not find CRITICALITY_*_LOGIT in ${sourcePath}`);
}
const protectionConstName = constMatch[1];

let ok = false;
try {
  for (const variant of variants) {
    const patched = originalSource.replace(
      constRe,
      `const ${protectionConstName}: f64 = ${rustFloat(variant.threshold)};`,
    );
    fs.writeFileSync(sourcePath, patched);
    console.error(`building ${variant.tag}: ${variant.threshold}`);
    run("cargo", ["build", "--release"]);
    fs.copyFileSync(path.resolve("target/release/boa"), variant.binary);
    fs.chmodSync(variant.binary, 0o755);
  }
  ok = true;
} finally {
  fs.writeFileSync(sourcePath, originalSource);
}

manifest.restored_source = true;
manifest.complete = ok;
const manifestPath = path.join(outDir, "manifest.json");
fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`wrote ${manifestPath}`);

function run(command, commandArgs) {
  const result = spawnSync(command, commandArgs, {stdio: "inherit"});
  if (result.error) fail(result.error.message);
  if (result.status !== 0) fail(`${command} ${commandArgs.join(" ")} exited with status ${result.status}`);
}

function percentileKey(percentile) {
  return `p${percentile}`.replace(".", "_");
}

function rustFloat(value) {
  return Number(value).toPrecision(17);
}

function timestamp() {
  const now = new Date();
  const pad = (n, w = 2) => String(n).padStart(w, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}_${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}${pad(now.getMilliseconds(), 3)}`;
}

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}
