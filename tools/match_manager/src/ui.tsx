import React, {useEffect, useMemo, useState} from "react";
import {Box, Text, useApp, useInput} from "ink";
import TextInput from "ink-text-input";
import {
  defaultSettings,
  engineLabel,
  importEngine,
  listEngines,
  MatchStore,
  snapshotEngine,
  STOCKFISH,
} from "./core.js";
import type {EngineSpec, GameDetail, GameRow, MatchConfig, MatchSettings, MatchSummary} from "./types.js";
import fs from "node:fs";

type Screen =
  | {name: "home"}
  | {name: "engines"}
  | {name: "snapshot"}
  | {name: "import"}
  | {name: "new-match"}
  | {name: "matches"}
  | {name: "detail"; id: string}
  | {name: "replay"; id: string; game: number};

type Notice = {kind: "ok" | "error"; text: string} | null;

const store = new MatchStore();

export function App(): React.ReactElement {
  const {exit} = useApp();
  const [screen, setScreen] = useState<Screen>({name: "home"});
  const [notice, setNotice] = useState<Notice>(null);
  const [, setRevision] = useState(0);

  useEffect(() => {
    store.load();
    const bump = () => setRevision((n) => n + 1);
    const interval = setInterval(bump, 1500);
    store.on("change", bump);
    const cleanup = () => store.stopAll();
    process.on("SIGINT", cleanup);
    process.on("SIGTERM", cleanup);
    return () => {
      clearInterval(interval);
      store.off("change", bump);
      process.off("SIGINT", cleanup);
      process.off("SIGTERM", cleanup);
    };
  }, []);

  const flash = (kind: "ok" | "error", text: string) => {
    setNotice({kind, text});
    setTimeout(() => setNotice(null), 3500);
  };

  const goHome = () => setScreen({name: "home"});

  return (
    <Box flexDirection="column">
      <Header />
      {notice && <NoticeLine notice={notice} />}
      {screen.name === "home" && <Home setScreen={setScreen} exit={exit} />}
      {screen.name === "engines" && <Engines goHome={goHome} setScreen={setScreen} flash={flash} />}
      {screen.name === "snapshot" && <Snapshot goBack={() => setScreen({name: "engines"})} flash={flash} />}
      {screen.name === "import" && <Import goBack={() => setScreen({name: "engines"})} flash={flash} />}
      {screen.name === "new-match" && <NewMatch setScreen={setScreen} flash={flash} />}
      {screen.name === "matches" && <Matches setScreen={setScreen} flash={flash} />}
      {screen.name === "detail" && <Detail id={screen.id} setScreen={setScreen} flash={flash} />}
      {screen.name === "replay" && <Replay id={screen.id} game={screen.game} setScreen={setScreen} flash={flash} />}
      <Help />
    </Box>
  );
}

function Header(): React.ReactElement {
  const summaries = store.summaries();
  const running = summaries.filter((m) => m.status === "running").length;
  return (
    <Box borderStyle="round" borderColor="yellow" paddingX={1} justifyContent="space-between">
      <Text color="yellow" bold>Boa Match Manager</Text>
      <Text color={running ? "green" : "gray"}>{running} running | {summaries.length} matches | {listEngines().length} engines</Text>
    </Box>
  );
}

function NoticeLine({notice}: {notice: Notice}): React.ReactElement | null {
  if (!notice) return null;
  return <Text color={notice.kind === "error" ? "red" : "green"}>{notice.text}</Text>;
}

function Help(): React.ReactElement {
  return (
    <Box marginTop={1}>
      <Text color="gray">↑/↓ navigate  enter select  b back  r refresh  q quit/back</Text>
    </Box>
  );
}

function Home({setScreen, exit}: {setScreen: (screen: Screen) => void; exit: () => void}): React.ReactElement {
  const items = [
    {label: "Matches", detail: "review, stop, delete, replay games", run: () => setScreen({name: "matches"})},
    {label: "New Match", detail: "launch cutechess with saved snapshots or Stockfish", run: () => setScreen({name: "new-match"})},
    {label: "Engine Library", detail: "snapshot, import, delete builds", run: () => setScreen({name: "engines"})},
    {label: "Quit", detail: "stop running matches and exit", run: exit},
  ];
  const [selected, setSelected] = useState(0);
  useInput((input, key) => {
    if (key.upArrow) setSelected((n) => Math.max(0, n - 1));
    if (key.downArrow) setSelected((n) => Math.min(items.length - 1, n + 1));
    if (key.return) items[selected].run();
    if (input === "q") exit();
  });
  return (
    <Box marginTop={1} gap={2}>
      <Box flexDirection="column" width={28} borderStyle="round" borderColor="gray" paddingX={1}>
        {items.map((item, index) => <MenuLine key={item.label} active={index === selected} label={item.label} />)}
      </Box>
      <Box flexDirection="column" flexGrow={1}>
        <Text bold>{items[selected].label}</Text>
        <Text color="gray">{items[selected].detail}</Text>
        <Box marginTop={1} flexDirection="column">
          <Text>{store.summaries().filter((m) => m.status === "running").length} running matches</Text>
          <Text>{listEngines().length} saved engine snapshots</Text>
          <Text>{fs.existsSync(STOCKFISH) ? "Stockfish detected" : "Stockfish not found"}</Text>
        </Box>
      </Box>
    </Box>
  );
}

function MenuLine({active, label}: {active: boolean; label: string}): React.ReactElement {
  return <Text color={active ? "yellow" : undefined}>{active ? ">" : " "} {label}</Text>;
}

function Engines({goHome, setScreen, flash}: {goHome: () => void; setScreen: (screen: Screen) => void; flash: (kind: "ok" | "error", text: string) => void}): React.ReactElement {
  const engines = listEngines();
  const [selected, setSelected] = useState(0);
  const [confirm, setConfirm] = useState(false);
  useInput((input, key) => {
    if (confirm) {
      if (key.return && engines[selected]) {
        try {
          store.deleteEngine(engines[selected].name);
          flash("ok", `Deleted ${engines[selected].name}`);
          setConfirm(false);
        } catch (error) {
          flash("error", error instanceof Error ? error.message : String(error));
        }
      }
      if (key.escape || input === "b" || input === "q") setConfirm(false);
      return;
    }
    if (key.upArrow) setSelected((n) => Math.max(0, n - 1));
    if (key.downArrow) setSelected((n) => Math.min(Math.max(engines.length - 1, 0), n + 1));
    if (input === "b" || input === "q") goHome();
    if (input === "s") setScreen({name: "snapshot"});
    if (input === "i") setScreen({name: "import"});
    if (input === "d" && engines[selected]) setConfirm(true);
    if (input === "r") store.load();
  });
  return (
    <Box marginTop={1} flexDirection="column">
      <Text bold>Engine Library</Text>
      <Text color="gray">s snapshot current build  i import binary  d delete selected</Text>
      <Box marginTop={1} flexDirection="column" borderStyle="round" borderColor={confirm ? "red" : "gray"} paddingX={1}>
        {engines.length === 0 && <Text color="gray">No snapshots yet.</Text>}
        {engines.map((engine, index) => (
          <Text key={engine.name} color={index === selected ? "yellow" : undefined}>
            {index === selected ? ">" : " "} {engine.name.padEnd(28)} <Text color="gray">{engine.created?.slice(0, 10) ?? ""} {engine.note ?? ""}</Text>
          </Text>
        ))}
      </Box>
      {confirm && engines[selected] && <Text color="red">Press enter to delete '{engines[selected].name}', or b to cancel.</Text>}
    </Box>
  );
}

function Snapshot({goBack, flash}: {goBack: () => void; flash: (kind: "ok" | "error", text: string) => void}): React.ReactElement {
  const [name, setName] = useState("");
  const [note, setNote] = useState("");
  const [field, setField] = useState<"name" | "note">("name");
  const [busy, setBusy] = useState(false);
  useInput((input, key) => {
    if (busy) return;
    if (key.tab || key.downArrow || key.upArrow) setField((f) => f === "name" ? "note" : "name");
    if (input === "b" || input === "q") goBack();
    if (key.return && name.trim()) {
      setBusy(true);
      snapshotEngine(name, note).then((meta) => {
        flash("ok", `Snapshot '${meta.name}' created`);
        goBack();
      }).catch((error) => flash("error", error instanceof Error ? error.message : String(error))).finally(() => setBusy(false));
    }
  });
  return (
    <Box marginTop={1} flexDirection="column">
      <Text bold>Snapshot Current Build</Text>
      <Text color="gray">Builds cargo --release from repo root and stores target/release/boa.</Text>
      <Field label="Name" active={field === "name"} value={field === "name" ? <TextInput value={name} onChange={setName} /> : name} />
      <Field label="Note" active={field === "note"} value={field === "note" ? <TextInput value={note} onChange={setNote} /> : note} />
      <Text color="yellow">{busy ? "Building..." : "Enter creates snapshot"}</Text>
    </Box>
  );
}

function Import({goBack, flash}: {goBack: () => void; flash: (kind: "ok" | "error", text: string) => void}): React.ReactElement {
  const [name, setName] = useState("");
  const [binary, setBinary] = useState("");
  const [note, setNote] = useState("");
  const fields = ["name", "binary", "note"] as const;
  const [index, setIndex] = useState(0);
  useInput((input, key) => {
    if (key.upArrow) setIndex((n) => Math.max(0, n - 1));
    if (key.downArrow || key.tab) setIndex((n) => Math.min(fields.length - 1, n + 1));
    if (input === "b" || input === "q") goBack();
    if (key.return && name.trim() && binary.trim()) {
      try {
        const meta = importEngine(name, binary, note);
        flash("ok", `Imported '${meta.name}'`);
        goBack();
      } catch (error) {
        flash("error", error instanceof Error ? error.message : String(error));
      }
    }
  });
  const active = fields[index];
  return (
    <Box marginTop={1} flexDirection="column">
      <Text bold>Import Existing Binary</Text>
      <Field label="Name" active={active === "name"} value={active === "name" ? <TextInput value={name} onChange={setName} /> : name} />
      <Field label="Path" active={active === "binary"} value={active === "binary" ? <TextInput value={binary} onChange={setBinary} /> : binary} />
      <Field label="Note" active={active === "note"} value={active === "note" ? <TextInput value={note} onChange={setNote} /> : note} />
      <Text color="yellow">Enter imports when name and path are set.</Text>
    </Box>
  );
}

function Field({label, active, value}: {label: string; active: boolean; value: React.ReactNode}): React.ReactElement {
  return (
    <Box>
      <Text color={active ? "yellow" : "gray"}>{active ? ">" : " "} {label.padEnd(12)} </Text>
      <Text>{value}</Text>
    </Box>
  );
}

function Matches({setScreen, flash}: {setScreen: (screen: Screen) => void; flash: (kind: "ok" | "error", text: string) => void}): React.ReactElement {
  const matches = store.summaries();
  const [selected, setSelected] = useState(0);
  const [confirmDelete, setConfirmDelete] = useState(false);
  useEffect(() => {
    setSelected((n) => Math.min(Math.max(0, matches.length - 1), n));
  }, [matches.length]);
  useInput((input, key) => {
    const match = matches[selected];
    if (confirmDelete) {
      if (key.return && match) {
        try {
          store.deleteMatch(match.id);
          flash("ok", `Deleted ${match.id}`);
          setConfirmDelete(false);
        } catch (error) {
          flash("error", error instanceof Error ? error.message : String(error));
        }
      }
      if (input === "b" || input === "q" || key.escape) setConfirmDelete(false);
      return;
    }
    if (key.upArrow || input === "k") setSelected((n) => Math.max(0, n - 1));
    if (key.downArrow || input === "j") setSelected((n) => Math.min(Math.max(matches.length - 1, 0), n + 1));
    if (key.pageUp) setSelected((n) => Math.max(0, n - 5));
    if (key.pageDown) setSelected((n) => Math.min(Math.max(matches.length - 1, 0), n + 5));
    if (input === "g") setSelected(0);
    if (input === "G") setSelected(Math.max(0, matches.length - 1));
    if (key.return && matches[selected]) setScreen({name: "detail", id: matches[selected].id});
    if (input === "s" && match) {
      const managed = store.get(match.id);
      if (managed?.status === "running") {
        managed.stop();
        flash("ok", `Stopped ${match.id}`);
      } else {
        flash("error", "Only running matches can be stopped");
      }
    }
    if (input === "d" && match) {
      if (match.status === "running") flash("error", "Stop the match before deleting it");
      else setConfirmDelete(true);
    }
    if (input === "n") setScreen({name: "new-match"});
    if (input === "r") store.load();
    if (input === "b" || input === "q") setScreen({name: "home"});
  });
  const windowStart = Math.max(0, Math.min(selected - 4, matches.length - 9));
  const visibleMatches = matches.slice(windowStart, windowStart + 9);
  return (
    <Box marginTop={1} flexDirection="column">
      <Box borderStyle="round" borderColor="cyan" paddingX={1} justifyContent="space-between">
        <Text bold color="cyan">Matches</Text>
        <Text color="gray">{matches.length ? `${selected + 1}/${matches.length}` : "0/0"}</Text>
      </Box>
      <Text color="gray">↑/↓ or j/k select  pgup/pgdn jump  g/G top/end  enter details  s stop  d delete  n new</Text>
      <Box marginTop={1} flexDirection="column" borderStyle="round" borderColor={confirmDelete ? "red" : "gray"} paddingX={1}>
        {matches.length === 0 && <Text color="gray">No matches yet.</Text>}
        {visibleMatches.map((match) => <MatchLine key={match.id} match={match} active={match.id === matches[selected]?.id} />)}
      </Box>
      {confirmDelete && matches[selected] && <Text color="red">Press enter to delete {matches[selected].id}, or b to cancel.</Text>}
    </Box>
  );
}

function MatchLine({match, active}: {match: MatchSummary; active: boolean}): React.ReactElement {
  const r = match.results;
  const line1 = `${active ? ">" : " "} ${match.id} ${match.status} +${r.wins} =${r.draws} -${r.losses} ${r.games_done}/${match.settings.games} Elo ${eloStr(r)}`;
  const line2 = `  ${clip(engineLabel(match.white), 30)} vs ${clip(engineLabel(match.black), 30)}  ${match.settings.tc}`;
  return (
    <Box flexDirection="column" marginBottom={1}>
      <Text color={active ? "yellow" : undefined}>{line1}</Text>
      <Text color="gray">{line2}</Text>
    </Box>
  );
}

function Status({status}: {status: string}): React.ReactElement {
  const color = status === "running" ? "green" : status === "error" ? "red" : status === "stopped" || status === "interrupted" ? "yellow" : "gray";
  return <Text color={color}>{status}</Text>;
}

function eloStr(r: {elo: number | null; elo_error: number | null}): string {
  if (r.elo == null) return "-";
  const sign = r.elo > 0 ? "+" : "";
  return `${sign}${r.elo}${r.elo_error != null ? ` +/-${r.elo_error}` : ""}`;
}

function Detail({id, setScreen, flash}: {id: string; setScreen: (screen: Screen) => void; flash: (kind: "ok" | "error", text: string) => void}): React.ReactElement {
  const match = store.get(id);
  const games = match?.gamesList() ?? [];
  const [selected, setSelected] = useState(0);
  const [confirmDelete, setConfirmDelete] = useState(false);
  useInput((input, key) => {
    if (!match) return setScreen({name: "matches"});
    if (confirmDelete) {
      if (key.return) {
        try {
          store.deleteMatch(id);
          flash("ok", `Deleted ${id}`);
          setScreen({name: "matches"});
        } catch (error) {
          flash("error", error instanceof Error ? error.message : String(error));
        }
      }
      if (input === "b" || input === "q" || key.escape) setConfirmDelete(false);
      return;
    }
    if (key.upArrow || input === "k") setSelected((n) => Math.max(0, n - 1));
    if (key.downArrow || input === "j") setSelected((n) => Math.min(Math.max(games.length - 1, 0), n + 1));
    if (key.pageUp) setSelected((n) => Math.max(0, n - 10));
    if (key.pageDown) setSelected((n) => Math.min(Math.max(games.length - 1, 0), n + 10));
    if (input === "g") setSelected(0);
    if (input === "G") setSelected(Math.max(0, games.length - 1));
    if (key.return && games[selected]) setScreen({name: "replay", id, game: games[selected].index});
    if (input === "s") {
      match.stop();
      flash("ok", `Stopped ${id}`);
    }
    if (input === "d") {
      if (match.status === "running") flash("error", "Stop the match before deleting it");
      else setConfirmDelete(true);
    }
    if (input === "b" || input === "q") setScreen({name: "matches"});
  });
  if (!match) return <Text color="red">No such match.</Text>;
  const summary = match.summary();
  const r = summary.results;
  const engineOne = engineOneName(summary);
  const windowStart = Math.max(0, Math.min(selected - 7, games.length - 15));
  const visibleGames = games.slice(windowStart, windowStart + 15);
  const selectedGame = games[selected];
  const compact = terminalWidth() < 116;
  const browserWidth = compact ? Math.max(58, terminalWidth() - 4) : 78;
  return (
    <Box marginTop={1} flexDirection="column">
      <Box borderStyle="round" borderColor="cyan" paddingX={1} flexDirection="column">
        <Box justifyContent="space-between">
          <Text bold color="cyan">{engineLabel(summary.white)} <Text color="gray">vs</Text> {engineLabel(summary.black)}</Text>
          <Status status={summary.status} />
        </Box>
        <Text color="gray">{summary.id}  •  {summary.settings.tc}  •  {summary.settings.concurrency} threads  •  hash {summary.settings.hash} MB</Text>
      </Box>
      <Text color="gray">↑/↓ or j/k select  pgup/pgdn jump  g/G top/end  enter replay  s stop  d delete</Text>
      <Box marginTop={1} gap={1}>
        <Stat label="Score" value={`+${r.wins} =${r.draws} -${r.losses}`} />
        <Stat label="Games" value={`${r.games_done}/${summary.settings.games}`} />
        <Stat label="Elo" value={eloStr(r)} color={r.elo && r.elo > 0 ? "green" : r.elo && r.elo < 0 ? "red" : undefined} />
        <Stat label="LOS" value={r.los == null ? "-" : `${r.los}%`} />
        {summary.settings.sprt && <Stat label="SPRT" value={`${r.sprt_result ?? "running"} ${r.sprt_llr ?? ""}`} />}
      </Box>
      {summary.error && <Text color="red">Error: {summary.error}</Text>}
      <ScoreBar wins={r.wins} draws={r.draws} losses={r.losses} width={48} />
      <Box marginTop={1} gap={2} flexDirection={compact ? "column" : "row"}>
        <Box flexDirection="column" width={browserWidth} borderStyle="round" borderColor="gray" paddingX={1}>
          <Box justifyContent="space-between">
            <Text bold>Game browser</Text>
            <Text color="gray">{games.length ? `${selected + 1}/${games.length}` : "0/0"}</Text>
          </Box>
          <Text color="gray">   #   Result    White                  Black                  Plies  End</Text>
          {games.length === 0 && <Text color="gray">No PGN games yet.</Text>}
          {visibleGames.map((game) => <GameLine key={game.index} game={game} active={game.index === selected} engineOne={engineOne} />)}
        </Box>
        <GamePreview game={selectedGame} width={compact ? browserWidth : 34} engineOne={engineOne} />
      </Box>
      {confirmDelete && <Text color="red">Press enter to delete this match and PGN, or b to cancel.</Text>}
    </Box>
  );
}

function GameLine({game, active, engineOne}: {game: GameRow; active: boolean; engineOne: string}): React.ReactElement {
  return (
    <Box>
      <Text color={active ? "yellow" : "gray"}>{active ? "▶" : " "} {(game.index + 1).toString().padStart(3)} </Text>
      <ResultPill result={game.result} active={active} outcome={gameOutcomeForEngineOne(game, engineOne)} />
      <Text color={active ? "white" : undefined}> {clip(game.white, 21)} <Text color="gray">vs</Text> {clip(game.black, 21)} </Text>
      <Text color="magenta">{game.plies.toString().padStart(4)}</Text>
      <Text color="gray">  {clip(cleanTermination(game.termination), 14)}</Text>
    </Box>
  );
}

function GamePreview({game, width, engineOne}: {game: GameRow | undefined; width: number; engineOne: string}): React.ReactElement {
  const outcome = game ? gameOutcomeForEngineOne(game, engineOne) : "unknown";
  return (
    <Box flexDirection="column" width={width} borderStyle="round" borderColor={game ? outcomeColor(outcome) : "gray"} paddingX={1}>
      <Text bold>{game ? `Game ${game.index + 1}` : "No game selected"}</Text>
      {game ? (
        <>
          <Box marginY={1}><ResultPill result={game.result} outcome={outcome} /></Box>
          <Text color="gray">Green/red is Engine 1's result</Text>
          <Text><Text color="gray">White </Text>{clip(game.white, 25)}</Text>
          <Text><Text color="gray">Black </Text>{clip(game.black, 25)}</Text>
          <Text><Text color="gray">Round </Text>{game.round}</Text>
          <Text><Text color="gray">Plies </Text>{game.plies}</Text>
          <Text><Text color="gray">End   </Text>{clip(cleanTermination(game.termination), 25)}</Text>
          <Box marginTop={1}><Text color="cyan">Press enter to open replay</Text></Box>
        </>
      ) : <Text color="gray">Completed games will appear here as PGN is written.</Text>}
    </Box>
  );
}

type GameOutcome = "win" | "loss" | "draw" | "pending" | "unknown";

function ResultPill({result, active = false, outcome}: {result: string; active?: boolean; outcome?: GameOutcome}): React.ReactElement {
  const label = result === "1-0" ? " 1-0 " : result === "0-1" ? " 0-1 " : result === "1/2-1/2" ? " ½-½ " : "  *  ";
  const color = outcome ? outcomeColor(outcome) : resultColor(result);
  return <Text color={active ? "black" : color} backgroundColor={active ? color : undefined}>{label}</Text>;
}

function gameOutcomeForEngineOne(game: GameRow, engineOne: string): GameOutcome {
  return outcomeFromPlayers(game.result, game.white, game.black, engineOne);
}

function outcomeFromPlayers(result: string, white: string | undefined, black: string | undefined, engineOne: string): GameOutcome {
  if (result === "*") return "pending";
  if (result === "1/2-1/2") return "draw";
  const winner = result === "1-0" ? white : result === "0-1" ? black : undefined;
  if (!winner) return "unknown";
  return sameEngineName(winner, engineOne) ? "win" : "loss";
}

function sameEngineName(a: string | undefined, b: string): boolean {
  return (a ?? "").trim() === b.trim();
}

function engineOneName(summary: MatchSummary): string {
  return summary.results.name1 ?? resolvedEngineName(summary.white);
}

function resolvedEngineName(spec: EngineSpec): string {
  return spec.type === "stockfish" ? `SF_${Number(spec.elo || 2000)}` : spec.name;
}

function outcomeColor(outcome: GameOutcome): string {
  if (outcome === "win") return "green";
  if (outcome === "loss") return "red";
  if (outcome === "draw") return "gray";
  if (outcome === "pending") return "yellow";
  return "gray";
}

function cleanTermination(text: string): string {
  return text.replace(/^normal$/i, "normal").replace(/ adjudication/i, " adj.");
}

function clip(text: string, width: number): string {
  if (text.length <= width) return text.padEnd(width);
  return `${text.slice(0, Math.max(0, width - 1))}…`;
}

function Stat({label, value, color}: {label: string; value: string; color?: string}): React.ReactElement {
  return (
    <Box borderStyle="round" borderColor="gray" paddingX={1} flexDirection="column" minWidth={12}>
      <Text color="gray">{label}</Text>
      <Text color={color}>{value}</Text>
    </Box>
  );
}

function ScoreBar({wins, draws, losses, width = 30}: {wins: number; draws: number; losses: number; width?: number}): React.ReactElement {
  const total = Math.max(1, wins + draws + losses);
  const chunk = (n: number, char: string) => char.repeat(Math.round(width * n / total));
  return (
    <Box marginTop={1}>
      <Text color="green">{chunk(wins, "█")}</Text>
      <Text color="gray">{chunk(draws, "█")}</Text>
      <Text color="red">{chunk(losses, "█")}</Text>
    </Box>
  );
}

function resultColor(result: string): string | undefined {
  if (result === "1-0") return "green";
  if (result === "0-1") return "red";
  if (result === "1/2-1/2") return "gray";
  if (result === "*") return "yellow";
  return undefined;
}

function resultBorder(result: string): string {
  return resultColor(result) ?? "gray";
}

function terminalWidth(): number {
  return process.stdout.columns || 120;
}

function NewMatch({setScreen, flash}: {setScreen: (screen: Screen) => void; flash: (kind: "ok" | "error", text: string) => void}): React.ReactElement {
  const engines = listEngines();
  const stockfish = fs.existsSync(STOCKFISH);
  const engineChoices = [...engines.map((e) => `snap:${e.name}`), ...(stockfish ? ["stockfish"] : [])];
  const first = engineChoices[0] ?? "";
  const [form, setForm] = useState<Record<string, string>>(() => ({
    engineA: first,
    engineB: engineChoices[1] ?? first,
    eloA: "2000",
    eloB: "2000",
    extraA: "",
    extraB: "",
    ...settingsToStrings(defaultSettings()),
  }));
  const fields = ["engineA", "eloA", "engineB", "eloB", "games", "tc", "concurrency", "hash", "openings", "adjudication", "sprt", "sprt_elo0", "sprt_elo1", "extraA", "extraB", "start"] as const;
  const [selected, setSelected] = useState(0);
  const [editing, setEditing] = useState<string | null>(null);
  useInput((input, key) => {
    if (editing) return;
    if (key.upArrow) setSelected((n) => Math.max(0, n - 1));
    if (key.downArrow || key.tab) setSelected((n) => Math.min(fields.length - 1, n + 1));
    if (input === "b" || input === "q") setScreen({name: "home"});
    const field = fields[selected];
    if (key.return) {
      if (field === "start") {
        try {
          const match = store.start(buildMatchConfig(form));
          flash("ok", `Started ${match.id}`);
          setScreen({name: "detail", id: match.id});
        } catch (error) {
          flash("error", error instanceof Error ? error.message : String(error));
        }
      } else if (["openings", "adjudication", "sprt"].includes(field)) {
        setForm((f) => ({...f, [field]: f[field] === "true" ? "false" : "true"}));
      } else {
        setEditing(field);
      }
    }
  });
  if (engineChoices.length === 0) {
    return (
      <Box marginTop={1} flexDirection="column">
        <Text color="red">No engines available. Snapshot or import a Boa build first.</Text>
        <Text color="gray">Press b to return.</Text>
      </Box>
    );
  }
  return (
    <Box marginTop={1} flexDirection="column">
      <Text bold>New Match</Text>
      <Text color="gray">Enter edits a field; booleans toggle. Engine fields cycle through available choices.</Text>
      {fields.map((field) => (
        <FormLine
          key={field}
          field={field}
          active={fields[selected] === field}
          editing={editing === field}
          value={field === "start" ? "Start match" : form[field]}
          choices={field === "engineA" || field === "engineB" ? engineChoices : undefined}
          onValue={(value) => setForm((f) => ({...f, [field]: value}))}
          done={() => setEditing(null)}
        />
      ))}
    </Box>
  );
}

function FormLine({
  field,
  active,
  editing,
  value,
  choices,
  onValue,
  done,
}: {
  field: string;
  active: boolean;
  editing: boolean;
  value: string;
  choices?: string[];
  onValue: (value: string) => void;
  done: () => void;
}): React.ReactElement {
  useInput((_, key) => {
    if (!editing || !choices) return;
    const index = choices.indexOf(value);
    if (key.leftArrow || key.upArrow) onValue(choices[Math.max(0, index - 1)] ?? choices[0]);
    if (key.rightArrow || key.downArrow) onValue(choices[Math.min(choices.length - 1, index + 1)] ?? choices[0]);
    if (key.return || key.escape) done();
  });
  const label = fieldLabel(field);
  const bool = value === "true" || value === "false";
  return (
    <Box>
      <Text color={active ? "yellow" : "gray"}>{active ? ">" : " "} {label.padEnd(15)} </Text>
      {editing && !choices && field !== "start" ? (
        <TextInput value={value} onChange={onValue} onSubmit={done} />
      ) : (
        <Text color={bool ? (value === "true" ? "green" : "red") : field === "start" ? "green" : undefined}>{displayValue(field, value)}</Text>
      )}
      {editing && choices && <Text color="gray">  ←/→ choose, enter done</Text>}
    </Box>
  );
}

function fieldLabel(field: string): string {
  return {
    engineA: "Engine A",
    eloA: "SF Elo A",
    engineB: "Engine B",
    eloB: "SF Elo B",
    games: "Games",
    tc: "Time control",
    concurrency: "Concurrency",
    hash: "Hash MB",
    openings: "Openings",
    adjudication: "Adjudication",
    sprt: "SPRT",
    sprt_elo0: "SPRT elo0",
    sprt_elo1: "SPRT elo1",
    extraA: "Extra A",
    extraB: "Extra B",
    start: "",
  }[field] ?? field;
}

function displayValue(field: string, value: string): string {
  if (field === "start") return value;
  if (field === "engineA" || field === "engineB") return value === "stockfish" ? "Stockfish" : value.replace(/^snap:/, "");
  if (value === "true") return "on";
  if (value === "false") return "off";
  return value || "-";
}

function settingsToStrings(settings: MatchSettings): Record<string, string> {
  return {
    games: String(settings.games),
    tc: settings.tc,
    concurrency: String(settings.concurrency),
    hash: String(settings.hash),
    openings: String(settings.openings),
    adjudication: String(settings.draw_adjudication && settings.resign_adjudication),
    sprt: String(settings.sprt),
    sprt_elo0: String(settings.sprt_elo0),
    sprt_elo1: String(settings.sprt_elo1),
  };
}

function buildEngine(value: string, elo: string, extra: string): EngineSpec {
  if (value === "stockfish") return {type: "stockfish", elo: Number(elo || 2000), extra_options: extra};
  return {type: "snapshot", name: value.replace(/^snap:/, ""), extra_options: extra};
}

function buildMatchConfig(form: Record<string, string>): MatchConfig {
  const adjudication = form.adjudication === "true";
  return {
    white: buildEngine(form.engineA, form.eloA, form.extraA),
    black: buildEngine(form.engineB, form.eloB, form.extraB),
    settings: {
      games: Number(form.games),
      tc: form.tc.trim(),
      concurrency: Number(form.concurrency),
      hash: Number(form.hash),
      openings: form.openings === "true",
      draw_adjudication: adjudication,
      resign_adjudication: adjudication,
      sprt: form.sprt === "true",
      sprt_elo0: Number(form.sprt_elo0),
      sprt_elo1: Number(form.sprt_elo1),
    },
  };
}

function Replay({id, game, setScreen, flash}: {id: string; game: number; setScreen: (screen: Screen) => void; flash: (kind: "ok" | "error", text: string) => void}): React.ReactElement {
  const match = store.get(id);
  const detail = useMemo<GameDetail | null>(() => match?.gameDetail(game) ?? null, [id, game, match]);
  const gameCount = match?.gamesList().length ?? 0;
  const engineOne = match ? engineOneName(match.summary()) : "";
  const [ply, setPly] = useState(0);
  const [pieceStyle, setPieceStyle] = useState<PieceStyle>("unicode");
  useEffect(() => setPly(0), [game]);
  useInput((input, key) => {
    if (!detail) {
      setScreen({name: "detail", id});
      return;
    }
    if (key.leftArrow || input === "h") setPly((n) => Math.max(0, n - 1));
    if (key.rightArrow || input === "l") setPly((n) => Math.min(detail.fens.length - 1, n + 1));
    if (key.upArrow || input === "k") setPly((n) => Math.max(0, n - 10));
    if (key.downArrow || input === "j") setPly((n) => Math.min(detail.fens.length - 1, n + 10));
    if (input === "0") setPly(0);
    if (input === "$") setPly(detail.fens.length - 1);
    if (input === "m") setPieceStyle(nextPieceStyle);
    if (input === "p" && game > 0) setScreen({name: "replay", id, game: game - 1});
    if (input === "n" && game + 1 < gameCount) setScreen({name: "replay", id, game: game + 1});
    if (input === "b" || input === "q") setScreen({name: "detail", id});
  });
  if (!detail) {
    flash("error", "No such game");
    return <Text color="red">No such game.</Text>;
  }
  const currentSan = ply === 0 ? "Initial position" : detail.sans[ply - 1] ?? "";
  const compact = terminalWidth() < 112;
  const outcome = outcomeFromPlayers(detail.headers.Result ?? "*", detail.headers.White, detail.headers.Black, engineOne);
  return (
    <Box marginTop={1} flexDirection="column">
      <Box borderStyle="round" borderColor={outcomeColor(outcome)} paddingX={1} flexDirection="column">
        <Box justifyContent="space-between">
          <Text bold>Game {game + 1}: {detail.headers.White} <Text color="gray">vs</Text> {detail.headers.Black}</Text>
          <ResultPill result={detail.headers.Result ?? "*"} outcome={outcome} />
        </Box>
        <Text color="gray">{detail.headers.Event ?? "PGN replay"}  •  round {detail.headers.Round ?? "?"}  •  {detail.headers.Termination ?? "normal"}  •  colors are Engine 1 result</Text>
      </Box>
      <Text color="gray">←/→ or h/l ply  ↑/↓ or j/k 10 plies  0/$ start/end  n/p game  m piece style  b back</Text>
      <Box marginTop={1} gap={2} flexDirection={compact ? "column" : "row"}>
        <Box flexDirection="column">
          <Board fen={detail.fens[ply]} pieceStyle={pieceStyle} />
          <Box marginTop={1} justifyContent="space-between" width={48}>
            <Text color="yellow">Ply {ply}/{detail.fens.length - 1}</Text>
            <Text color="cyan">{currentSan}</Text>
          </Box>
          <PlyTimeline ply={ply} total={detail.fens.length - 1} />
        </Box>
        <Box flexDirection="column" width={compact ? Math.max(44, terminalWidth() - 4) : 44} borderStyle="round" borderColor="gray" paddingX={1}>
          <Box justifyContent="space-between">
            <Text bold>Moves</Text>
            <Text color="gray">{Math.ceil(detail.sans.length / 2)} moves</Text>
          </Box>
          <MoveList sans={detail.sans} ply={ply} />
        </Box>
        <Box flexDirection="column" width={compact ? Math.max(44, terminalWidth() - 4) : 28} borderStyle="round" borderColor="gray" paddingX={1}>
          <Text bold>Position</Text>
          <Text><Text color="gray">Turn </Text>{sideToMove(detail.fens[ply])}</Text>
          <Text><Text color="gray">FEN</Text></Text>
          <Text color="gray">{clip(detail.fens[ply], 24)}</Text>
          <Box marginTop={1}>
            <Text color={game > 0 ? "cyan" : "gray"}>p previous</Text>
          </Box>
          <Text color={game + 1 < gameCount ? "cyan" : "gray"}>n next</Text>
        </Box>
      </Box>
    </Box>
  );
}

function MoveList({sans, ply}: {sans: string[]; ply: number}): React.ReactElement {
  const row = Math.max(0, Math.ceil(ply / 2) - 8);
  const rows = [];
  for (let i = row; i < Math.min(Math.ceil(sans.length / 2), row + 17); i++) rows.push(i);
  return (
    <Box flexDirection="column">
      {rows.map((i) => {
        const whitePly = i * 2 + 1;
        const blackPly = i * 2 + 2;
        return (
          <Box key={i}>
            <Text color="gray">{(i + 1).toString().padStart(3)}. </Text>
            <MoveText san={sans[i * 2] ?? ""} active={ply === whitePly} />
            <MoveText san={sans[i * 2 + 1] ?? ""} active={ply === blackPly} />
          </Box>
        );
      })}
    </Box>
  );
}

function MoveText({san, active}: {san: string; active: boolean}): React.ReactElement {
  return <Text color={active ? "black" : undefined} backgroundColor={active ? "yellow" : undefined}>{clip(san, 14)} </Text>;
}

function PlyTimeline({ply, total}: {ply: number; total: number}): React.ReactElement {
  const width = 48;
  const filled = total === 0 ? 0 : Math.round(width * ply / total);
  return (
    <Box width={width}>
      <Text color="cyan">{"█".repeat(filled)}</Text>
      <Text color="gray">{"░".repeat(Math.max(0, width - filled))}</Text>
    </Box>
  );
}

function sideToMove(fen: string): string {
  return fen.split(" ")[1] === "b" ? "Black" : "White";
}

const pieceLetters = new Set(["K", "Q", "R", "B", "N", "P", "k", "q", "r", "b", "n", "p"]);

const pieceGlyphs: Record<string, string> = {
  K: "♔︎", Q: "♕︎", R: "♖︎", B: "♗︎", N: "♘︎", P: "♙︎",
  k: "♚︎", q: "♛︎", r: "♜︎", b: "♝︎", n: "♞︎", p: "♟︎",
};

type PieceStyle = "unicode" | "text";

function nextPieceStyle(style: PieceStyle): PieceStyle {
  return style === "unicode" ? "text" : "unicode";
}

function Board({fen, pieceStyle}: {fen: string; pieceStyle: PieceStyle}): React.ReactElement {
  const rows = fen.split(" ")[0].split("/");
  const files = ["a", "b", "c", "d", "e", "f", "g", "h"];
  const cellWidth = 6;
  const fileHeader = `     ${files.map((file) => file.padStart(3).padEnd(cellWidth)).join("")}`;
  return (
    <Box flexDirection="column" borderStyle="double" borderColor="cyan" paddingX={1}>
      <Text color="gray">{fileHeader}</Text>
      {rows.map((row, r) => {
        const cells: string[] = [];
        for (const ch of row) {
          if (/\d/.test(ch)) {
            for (let i = 0; i < Number(ch); i++) cells.push(" ");
          } else {
            cells.push(ch);
          }
        }
        const rank = 8 - r;
        return (
          <Box key={r} flexDirection="column">
            <Box>
              <Text color="gray"> {rank}  </Text>
              {cells.map((cell, f) => <BoardSquare key={`${r}-${f}-top`} piece=" " light={(r + f) % 2 === 0} pieceStyle={pieceStyle} />)}
              <Text color="gray">  {rank}</Text>
            </Box>
            <Box>
              <Text color="gray">    </Text>
              {cells.map((cell, f) => <BoardSquare key={`${r}-${f}-piece`} piece={cell} light={(r + f) % 2 === 0} pieceStyle={pieceStyle} />)}
              <Text color="gray">  {rank}</Text>
            </Box>
            <Box>
              <Text color="gray">    </Text>
              {cells.map((_, f) => <BoardSquare key={`${r}-${f}-pad`} piece=" " light={(r + f) % 2 === 0} pieceStyle={pieceStyle} />)}
            </Box>
          </Box>
        );
      })}
      <Text color="gray">{fileHeader}</Text>
    </Box>
  );
}

function BoardSquare({piece, light, pieceStyle}: {piece: string; light: boolean; pieceStyle: PieceStyle}): React.ReactElement {
  const backgroundColor = light ? "rgb(172,154,124)" : "rgb(88,124,142)";
  const isPiece = pieceLetters.has(piece);
  const isWhite = isPiece && piece === piece.toUpperCase();
  const color = !isPiece ? undefined : isWhite ? "rgb(255,250,232)" : "rgb(24,20,18)";
  return (
    <Text backgroundColor={backgroundColor} color={color} bold={isPiece}>
      {pieceSquareText(piece, pieceStyle, isWhite, light)}
    </Text>
  );
}

function pieceSquareText(piece: string, pieceStyle: PieceStyle, isWhite: boolean, light: boolean): string {
  if (!pieceLetters.has(piece)) return "      ";
  if (pieceStyle === "unicode") return `  ${pieceGlyphs[piece]}   `;
  return `  ${isWhite ? "W" : "B"}${piece.toUpperCase()}  `;
}
