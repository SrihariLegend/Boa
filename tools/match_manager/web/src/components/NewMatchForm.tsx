import {useEffect, useState} from "react";
import {api} from "../api";
import type {Navigate} from "../App";
import type {EngineMeta, EngineSpec, MatchSettings} from "../types";

const defaultSettings: MatchSettings = {
  games: 100,
  tc: "10+0.1",
  concurrency: 4,
  hash: 128,
  openings: true,
  draw_adjudication: true,
  resign_adjudication: true,
  sprt: false,
  sprt_elo0: 0,
  sprt_elo1: 5,
};

export function NewMatchForm({engines, navigate, refresh}: {engines: EngineMeta[]; navigate: Navigate; refresh: () => void}) {
  const [white, setWhite] = useState<EngineSpec>({type: "snapshot", name: engines[0]?.name ?? ""});
  const [black, setBlack] = useState<EngineSpec>({type: "stockfish", elo: 2000});
  const [settings, setSettings] = useState(defaultSettings);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (white.type === "snapshot" && !white.name && engines[0]) setWhite({...white, name: engines[0].name});
  }, [engines, white]);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const response = await api.createMatch({white, black, settings});
      refresh();
      navigate(`/matches/${response.id}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="panel page-stack" onSubmit={submit}>
      <div className="section-heading"><div><p className="eyebrow">Launch cutechess</p><h1>New Match</h1></div><button className="primary" disabled={busy}>{busy ? "Starting…" : "Start Match"}</button></div>
      {error && <div className="alert">{error}</div>}
      {engines.length === 0 && <div className="alert">No engine snapshots found. Import or snapshot an engine first.</div>}
      <section className="form-grid">
        <EngineEditor title="Engine 1" value={white} engines={engines} onChange={setWhite} />
        <EngineEditor title="Engine 2" value={black} engines={engines} onChange={setBlack} />
      </section>
      <section className="form-grid settings-grid">
        <NumberInput label="Games" value={settings.games} min={2} onChange={(games) => setSettings({...settings, games})} />
        <TextInput label="Time control" value={settings.tc} onChange={(tc) => setSettings({...settings, tc})} />
        <NumberInput label="Concurrency" value={settings.concurrency} min={1} onChange={(concurrency) => setSettings({...settings, concurrency})} />
        <NumberInput label="Hash MB" value={settings.hash} min={1} onChange={(hash) => setSettings({...settings, hash})} />
        <Check label="Openings" value={settings.openings} onChange={(openings) => setSettings({...settings, openings})} />
        <Check label="Draw adjudication" value={settings.draw_adjudication} onChange={(draw_adjudication) => setSettings({...settings, draw_adjudication})} />
        <Check label="Resign adjudication" value={settings.resign_adjudication} onChange={(resign_adjudication) => setSettings({...settings, resign_adjudication})} />
        <Check label="SPRT" value={settings.sprt} onChange={(sprt) => setSettings({...settings, sprt})} />
        <NumberInput label="SPRT Elo0" value={settings.sprt_elo0} onChange={(sprt_elo0) => setSettings({...settings, sprt_elo0})} />
        <NumberInput label="SPRT Elo1" value={settings.sprt_elo1} onChange={(sprt_elo1) => setSettings({...settings, sprt_elo1})} />
      </section>
      <section className="panel inset">
        <h2>Preview</h2>
        <p className="muted">{describeEngine(white)} vs {describeEngine(black)} · {settings.games} games · {settings.tc} · concurrency {settings.concurrency}</p>
      </section>
    </form>
  );
}

function EngineEditor({title, value, engines, onChange}: {title: string; value: EngineSpec; engines: EngineMeta[]; onChange: (value: EngineSpec) => void}) {
  return (
    <section className="panel inset">
      <h2>{title}</h2>
      <label>Type<select value={value.type} onChange={(event) => onChange(event.target.value === "stockfish" ? {type: "stockfish", elo: 2000} : {type: "snapshot", name: engines[0]?.name ?? ""})}><option value="snapshot">Snapshot</option><option value="stockfish">Stockfish</option></select></label>
      {value.type === "snapshot" ? <label>Snapshot<select value={value.name} onChange={(event) => onChange({...value, name: event.target.value})}>{engines.map((engine) => <option key={engine.name} value={engine.name}>{engine.name}</option>)}</select></label> : <NumberInput label="Stockfish Elo" value={value.elo} onChange={(elo) => onChange({...value, elo})} />}
      <label>Extra UCI options<input value={value.extra_options ?? ""} onChange={(event) => onChange({...value, extra_options: event.target.value})} placeholder="Threads=1,Hash=64" /></label>
    </section>
  );
}

function describeEngine(engine: EngineSpec): string {
  return engine.type === "stockfish" ? `Stockfish ${engine.elo}` : engine.name || "snapshot";
}

function TextInput({label, value, onChange}: {label: string; value: string; onChange: (value: string) => void}) {
  return <label>{label}<input value={value} onChange={(event) => onChange(event.target.value)} /></label>;
}

function NumberInput({label, value, min, onChange}: {label: string; value: number; min?: number; onChange: (value: number) => void}) {
  return <label>{label}<input type="number" min={min} value={value} onChange={(event) => onChange(Number(event.target.value))} /></label>;
}

function Check({label, value, onChange}: {label: string; value: boolean; onChange: (value: boolean) => void}) {
  return <label className="check"><input type="checkbox" checked={value} onChange={(event) => onChange(event.target.checked)} />{label}</label>;
}
