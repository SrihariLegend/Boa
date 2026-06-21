import {useState} from "react";
import {api} from "../api";
import type {EngineMeta} from "../types";
import {dateShort} from "./helpers";

export function EngineLibrary({engines, refresh}: {engines: EngineMeta[]; refresh: () => void}) {
  const [snapshotName, setSnapshotName] = useState("");
  const [snapshotNote, setSnapshotNote] = useState("");
  const [importName, setImportName] = useState("");
  const [importPath, setImportPath] = useState("");
  const [importNote, setImportNote] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = async (label: string, fn: () => Promise<unknown>) => {
    setBusy(label);
    setError(null);
    setMessage(null);
    try {
      await fn();
      setMessage(`${label} complete`);
      refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="page-stack">
      <section className="panel">
        <div className="section-heading"><div><p className="eyebrow">Snapshots</p><h1>Engine Library</h1></div><span className="muted">{engines.length} engines</span></div>
        {message && <div className="notice">{message}</div>}
        {error && <div className="alert">{error}</div>}
        <div className="table-wrap">
          <table>
            <thead><tr><th>Name</th><th>Note</th><th>Created</th><th>Source</th><th>Actions</th></tr></thead>
            <tbody>
              {engines.map((engine) => (
                <tr key={engine.name}>
                  <td><strong>{engine.name}</strong></td>
                  <td>{engine.note || "—"}</td>
                  <td>{dateShort(engine.created)}</td>
                  <td className="source-cell">{engine.source || "—"}</td>
                  <td><button disabled={busy !== null} onClick={() => {
                    if (confirm(`Delete engine snapshot ${engine.name}?`)) void run("Delete", () => api.deleteEngine(engine.name));
                  }}>Delete</button></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {engines.length === 0 && <p className="muted empty-state">No snapshots found.</p>}
      </section>

      <section className="form-grid">
        <form className="panel" onSubmit={(event) => {
          event.preventDefault();
          void run("Snapshot", () => api.snapshotEngine(snapshotName, snapshotNote));
        }}>
          <h2>Snapshot Current Boa</h2>
          <p className="muted">Builds <code>cargo build --release</code> and stores a copy in the engine library.</p>
          <label>Name<input value={snapshotName} onChange={(event) => setSnapshotName(event.target.value)} placeholder="boa-new-eval" required /></label>
          <label>Note<input value={snapshotNote} onChange={(event) => setSnapshotNote(event.target.value)} placeholder="after eval tweak" /></label>
          <button className="primary" disabled={busy !== null}>{busy === "Snapshot" ? "Building…" : "Snapshot Current Build"}</button>
        </form>

        <form className="panel" onSubmit={(event) => {
          event.preventDefault();
          void run("Import", () => api.importEngine(importName, importPath, importNote));
        }}>
          <h2>Import Existing Binary</h2>
          <p className="muted">Copies a local UCI engine binary into the managed snapshot directory.</p>
          <label>Name<input value={importName} onChange={(event) => setImportName(event.target.value)} placeholder="external-build" required /></label>
          <label>Binary path<input value={importPath} onChange={(event) => setImportPath(event.target.value)} placeholder="/path/to/boa" required /></label>
          <label>Note<input value={importNote} onChange={(event) => setImportNote(event.target.value)} placeholder="manual build" /></label>
          <button disabled={busy !== null}>Import Engine</button>
        </form>
      </section>
    </div>
  );
}
