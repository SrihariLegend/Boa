#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import {
  DEFAULT_CONCURRENCY,
  ManagedMatch,
  MATCHES_DIR,
  defaultSettings,
  safeName,
  timestamp,
  writeJson,
} from "./core.js";
import type {MatchConfig, MatchResults, MatchSettings} from "./types.js";

type Ablation = {
  name: string;
  option: string;
  value: string;
  reason: string;
};

const ABLATIONS: Ablation[] = [
  {
    name: "no_eval_pst",
    option: "Eval PST Scale",
    value: "0",
    reason: "piece-square table contribution",
  },
  {
    name: "no_eval_mobility",
    option: "Eval Mobility Scale",
    value: "0",
    reason: "classical mobility/activity eval",
  },
  {
    name: "no_eval_pawn_structure",
    option: "Eval Pawn Structure Scale",
    value: "0",
    reason: "isolated/doubled/backward/passed pawn eval",
  },
  {
    name: "no_eval_king_safety",
    option: "Eval King Safety Scale",
    value: "0",
    reason: "king shield and king-zone attacks",
  },
  {
    name: "no_eval_freedom",
    option: "Eval Freedom Scale",
    value: "0",
    reason: "Boa freedom/squeeze metric",
  },
  {
    name: "no_eval_trade_down",
    option: "Eval Trade Down Scale",
    value: "0",
    reason: "simplification bonus when ahead",
  },
  {
    name: "no_eval_weak_squares",
    option: "Eval Weak Squares Scale",
    value: "0",
    reason: "hole and weak-square complex eval",
  },
  {
    name: "no_eval_coordination",
    option: "Eval Coordination Scale",
    value: "0",
    reason: "piece harmony, central overlap, spread",
  },
  {
    name: "no_eval_advanced_pawns",
    option: "Eval Advanced Pawns Scale",
    value: "0",
    reason: "advanced pawn eval",
  },
  {
    name: "no_search_lazy_smp",
    option: "Search Lazy SMP",
    value: "false",
    reason: "Lazy SMP worker search when Threads is greater than 1",
  },
  {
    name: "no_search_see",
    option: "Search SEE",
    value: "false",
    reason: "all static exchange evaluation search uses",
  },
  {
    name: "no_search_see_qsearch_pruning",
    option: "Search SEE QSearch Pruning",
    value: "false",
    reason: "SEE-based pruning of losing captures in qsearch",
  },
  {
    name: "no_search_see_capture_ordering",
    option: "Search SEE Capture Ordering",
    value: "false",
    reason: "SEE contribution to capture move ordering",
  },
  {
    name: "no_search_restriction_ordering",
    option: "Search Restriction Ordering",
    value: "false",
    reason: "quiet move ordering bonus for restriction",
  },
  {
    name: "no_search_squeeze_extensions",
    option: "Search Squeeze Extensions",
    value: "false",
    reason: "extra ply for squeeze-preserving moves",
  },
  {
    name: "no_search_squeeze_null_suppression",
    option: "Search Squeeze Null Move Suppression",
    value: "false",
    reason: "disable null-move pruning inside squeeze mode",
  },
  {
    name: "no_search_squeeze_lmr_relief",
    option: "Search Squeeze LMR Relief",
    value: "false",
    reason: "reduce LMR less in squeeze mode",
  },
];

const SCALE_SWEEPS: Ablation[] = [
  {
    name: "freedom_scale_0",
    option: "Eval Freedom Scale",
    value: "0",
    reason: "remove Boa freedom/squeeze eval; baseline is scale 100",
  },
  {
    name: "freedom_scale_50",
    option: "Eval Freedom Scale",
    value: "50",
    reason: "halve Boa freedom/squeeze eval; baseline is scale 100",
  },
  {
    name: "freedom_scale_150",
    option: "Eval Freedom Scale",
    value: "150",
    reason: "increase Boa freedom/squeeze eval; baseline is scale 100",
  },
  {
    name: "weak_squares_scale_0",
    option: "Eval Weak Squares Scale",
    value: "0",
    reason: "remove weak-square eval; baseline is scale 100",
  },
  {
    name: "weak_squares_scale_50",
    option: "Eval Weak Squares Scale",
    value: "50",
    reason: "halve weak-square eval; baseline is scale 100",
  },
  {
    name: "weak_squares_scale_150",
    option: "Eval Weak Squares Scale",
    value: "150",
    reason: "increase weak-square eval; baseline is scale 100",
  },
  {
    name: "coordination_scale_0",
    option: "Eval Coordination Scale",
    value: "0",
    reason: "remove coordination eval; baseline is scale 100",
  },
  {
    name: "coordination_scale_50",
    option: "Eval Coordination Scale",
    value: "50",
    reason: "halve coordination eval; baseline is scale 100",
  },
  {
    name: "coordination_scale_150",
    option: "Eval Coordination Scale",
    value: "150",
    reason: "increase coordination eval; baseline is scale 100",
  },
  {
    name: "advanced_pawns_scale_0",
    option: "Eval Advanced Pawns Scale",
    value: "0",
    reason: "remove advanced-pawn eval; baseline is scale 100",
  },
  {
    name: "advanced_pawns_scale_50",
    option: "Eval Advanced Pawns Scale",
    value: "50",
    reason: "halve advanced-pawn eval; baseline is scale 100",
  },
  {
    name: "advanced_pawns_scale_150",
    option: "Eval Advanced Pawns Scale",
    value: "150",
    reason: "increase advanced-pawn eval; baseline is scale 100",
  },
  {
    name: "restriction_ordering_scale_0",
    option: "Search Restriction Ordering Scale",
    value: "0",
    reason: "remove restriction move-ordering score; baseline is scale 100",
  },
  {
    name: "restriction_ordering_scale_25",
    option: "Search Restriction Ordering Scale",
    value: "25",
    reason: "quarter restriction move-ordering score; baseline is scale 100",
  },
  {
    name: "restriction_ordering_scale_50",
    option: "Search Restriction Ordering Scale",
    value: "50",
    reason: "halve restriction move-ordering score; baseline is scale 100",
  },
  {
    name: "restriction_ordering_scale_75",
    option: "Search Restriction Ordering Scale",
    value: "75",
    reason: "reduce restriction move-ordering score; baseline is scale 100",
  },
];

type CliOptions = {
  engine: string;
  suite: "ablation" | "scale";
  games: number;
  tc: string;
  concurrency: number;
  hash: number;
  sprt: boolean;
  sprtElo0: number;
  sprtElo1: number;
  openings: boolean;
  only: Set<string> | null;
  list: boolean;
};

type AblationSummary = {
  id: string;
  ablation: Ablation;
  results: MatchResults;
  verdict: string;
};

async function main(): Promise<void> {
  const cli = parseArgs(process.argv.slice(2));
  const experiments = experimentsForSuite(cli.suite);
  if (cli.list) {
    for (const ablation of experiments) {
      console.log(`${ablation.name}: ${ablation.option}=${ablation.value} (${ablation.reason})`);
    }
    return;
  }

  if (!cli.engine) {
    throw new Error("Pass --engine SNAPSHOT_NAME. Use Match Manager to snapshot the current binary first.");
  }

  const selected = experiments.filter((ablation) => !cli.only || cli.only.has(ablation.name));
  if (selected.length === 0) {
    throw new Error("No ablations selected. Run with --list to see names.");
  }

  const suiteId = `${cli.suite}_${timestamp()}`;
  const summaries: AblationSummary[] = [];

  console.log(`Ablation suite ${suiteId}`);
  console.log(`engine=${cli.engine} games=${cli.games} tc=${cli.tc} concurrency=${cli.concurrency}`);

  for (const ablation of selected) {
    const id = uniqueMatchId(`${suiteId}_${safeName(ablation.name)}`);
    const config = buildConfig(cli, ablation);
    const match = new ManagedMatch(id, config);

    console.log(`\nStarting ${ablation.name}: ${ablation.option}=${ablation.value}`);
    await runMatch(match);

    const verdict = verdictFor(match.results);
    summaries.push({
      id,
      ablation,
      results: {...match.results},
      verdict,
    });
    printResult(ablation.name, match.results, verdict);
  }

  const summaryPath = path.join(MATCHES_DIR, `${suiteId}_summary.json`);
  writeJson(summaryPath, summaries);
  console.log(`\nWrote ${summaryPath}`);
}

function parseArgs(args: string[]): CliOptions {
  const cli: CliOptions = {
    engine: "",
    suite: "ablation",
    games: 400,
    tc: "5+0.05",
    concurrency: DEFAULT_CONCURRENCY,
    hash: 128,
    sprt: false,
    sprtElo0: 0,
    sprtElo1: 5,
    openings: true,
    only: null,
    list: false,
  };

  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    const next = () => {
      const value = args[i + 1];
      if (value == null) throw new Error(`${arg} requires a value`);
      i += 1;
      return value;
    };

    switch (arg) {
      case "--engine":
        cli.engine = safeName(next());
        break;
      case "--suite": {
        const suite = next();
        if (suite !== "ablation" && suite !== "scale") {
          throw new Error("--suite must be 'ablation' or 'scale'");
        }
        cli.suite = suite;
        break;
      }
      case "--games":
        cli.games = Math.max(2, Number(next()));
        break;
      case "--tc":
        cli.tc = next();
        break;
      case "--concurrency":
        cli.concurrency = Math.max(1, Number(next()));
        break;
      case "--hash":
        cli.hash = Math.max(1, Number(next()));
        break;
      case "--sprt":
        cli.sprt = true;
        break;
      case "--sprt-elo0":
        cli.sprtElo0 = Number(next());
        break;
      case "--sprt-elo1":
        cli.sprtElo1 = Number(next());
        break;
      case "--only":
        cli.only = new Set(next().split(",").map((name) => name.trim()).filter(Boolean));
        break;
      case "--no-openings":
        cli.openings = false;
        break;
      case "--list":
        cli.list = true;
        break;
      case "--help":
        printHelp();
        process.exit(0);
      default:
        throw new Error(`Unknown option: ${arg}`);
    }
  }

  return cli;
}

function experimentsForSuite(suite: CliOptions["suite"]): Ablation[] {
  return suite === "scale" ? SCALE_SWEEPS : ABLATIONS;
}

function buildConfig(cli: CliOptions, ablation: Ablation): MatchConfig {
  const settings: MatchSettings = {
    ...defaultSettings(),
    games: cli.games,
    tc: cli.tc,
    concurrency: cli.concurrency,
    hash: cli.hash,
    openings: cli.openings,
    sprt: cli.sprt,
    sprt_elo0: cli.sprtElo0,
    sprt_elo1: cli.sprtElo1,
  };
  return {
    white: {
      type: "snapshot",
      name: cli.engine,
      extra_options: `${ablation.option}=${ablation.value}`,
    },
    black: {
      type: "snapshot",
      name: cli.engine,
      extra_options: "",
    },
    settings,
  };
}

function uniqueMatchId(base: string): string {
  let id = base;
  while (fs.existsSync(path.join(MATCHES_DIR, id))) id = `${id}x`;
  return id;
}

function runMatch(match: ManagedMatch): Promise<void> {
  return new Promise((resolve) => {
    match.on("change", () => {
      if (!["pending", "running"].includes(match.status)) resolve();
    });
    match.start();
  });
}

function verdictFor(results: MatchResults): string {
  if (results.elo == null) return "unknown";
  if (results.elo_error != null && Math.abs(results.elo) <= results.elo_error) {
    return "unclear";
  }
  return results.elo < 0 ? "keep term" : "suspect term";
}

function printResult(name: string, results: MatchResults, verdict: string): void {
  const elo = results.elo == null ? "?" : `${results.elo}`;
  const err = results.elo_error == null ? "" : ` +/-${results.elo_error}`;
  console.log(
    `${name}: +${results.wins} =${results.draws} -${results.losses} ` +
      `games=${results.games_done} Elo ${elo}${err} -> ${verdict}`,
  );
}

function printHelp(): void {
  console.log(`Usage:
  npm run ablate -- --engine SNAPSHOT [options]

Options:
  --list                 Print available ablations
  --suite NAME           ablation or scale (default ablation)
  --only A,B             Run only selected ablation names
  --games N              Total requested games per ablation (default 400)
  --tc TC                cutechess time control (default 5+0.05)
  --concurrency N        cutechess concurrency (default CPU-based)
  --hash N               Boa hash MB (default 128)
  --sprt                 Enable SPRT with elo0=0, elo1=5
  --sprt-elo0 N          SPRT lower hypothesis
  --sprt-elo1 N          SPRT upper hypothesis
  --no-openings          Disable openings.epd
`);
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
