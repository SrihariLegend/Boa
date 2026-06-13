/* ── Karpov Match Manager frontend ── vanilla JS, no deps ── */

const $ = (sel) => document.querySelector(sel);

let state = { engines: [], matches: [] };
let currentMatchId = null;   // non-null → detail view open
let replay = null;           // { fens, sans, ply, title }

// ── helpers ──────────────────────────────────────────────────────────────────

async function api(path, opts = {}) {
  const res = await fetch(path, {
    headers: { "Content-Type": "application/json" },
    ...opts,
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}

function toast(msg, isError = false) {
  const el = $("#toast");
  el.textContent = msg;
  el.className = isError ? "error" : "";
  el.classList.remove("hidden");
  clearTimeout(toast._t);
  toast._t = setTimeout(() => el.classList.add("hidden"), 4000);
}

function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
}

function engineLabel(spec) {
  return spec.type === "stockfish" ? `SF@${spec.elo}` : spec.name;
}

function eloStr(r) {
  if (r.elo === null || r.elo === undefined) return "—";
  const sign = r.elo > 0 ? "+" : "";
  const err = r.elo_error != null ? ` ±${r.elo_error}` : "";
  return `${sign}${r.elo}${err}`;
}

// ── polling ──────────────────────────────────────────────────────────────────

async function poll() {
  try {
    state = await api("/api/state");
    $("#conn-status").classList.add("ok");
    renderEngines();
    renderMatchList();
    if (currentMatchId) await refreshDetail();
  } catch {
    $("#conn-status").classList.remove("ok");
  }
}
setInterval(poll, 2000);

// ── engine library ───────────────────────────────────────────────────────────

function renderEngines() {
  const ul = $("#engine-list");
  if (!state.engines.length) {
    ul.innerHTML = `<li class="empty">No snapshots yet — build one below.</li>`;
  } else {
    ul.innerHTML = state.engines.map((e) => `
      <li>
        <span class="ename">${esc(e.name)}</span>
        <span class="enote">${esc(e.note || "")}</span>
        <span class="edate">${esc((e.created || "").slice(0, 10))}</span>
        <button class="edel" data-name="${esc(e.name)}" title="Delete snapshot">✕</button>
      </li>`).join("");
    ul.querySelectorAll(".edel").forEach((btn) => btn.onclick = async (ev) => {
      ev.stopPropagation();
      if (!confirm(`Delete snapshot '${btn.dataset.name}'?`)) return;
      try { await api(`/api/engines/${btn.dataset.name}`, { method: "DELETE" }); poll(); }
      catch (e) { toast(e.message, true); }
    });
  }
  // keep the two match-form dropdowns in sync with the library
  for (const id of ["engine-a", "engine-b"]) {
    const sel = document.getElementById(id);
    const prev = sel.value;
    sel.innerHTML =
      state.engines.map((e) => `<option value="snap:${esc(e.name)}">${esc(e.name)}</option>`).join("") +
      (state.stockfish_available ? `<option value="stockfish">Stockfish @ Elo…</option>` : "");
    if ([...sel.options].some((o) => o.value === prev)) sel.value = prev;
    sel.onchange = () => {
      const eloInput = document.getElementById(id === "engine-a" ? "elo-a" : "elo-b");
      eloInput.classList.toggle("hidden", sel.value !== "stockfish");
    };
  }
}

$("#snapshot-form").onsubmit = async (ev) => {
  ev.preventDefault();
  const btn = $("#snap-btn");
  btn.disabled = true;
  btn.textContent = "Building…";
  try {
    const meta = await api("/api/engines/snapshot", {
      method: "POST",
      body: JSON.stringify({ name: $("#snap-name").value, note: $("#snap-note").value }),
    });
    toast(`Snapshot '${meta.name}' created`);
    $("#snap-name").value = "";
    $("#snap-note").value = "";
    poll();
  } catch (e) { toast(e.message, true); }
  btn.disabled = false;
  btn.textContent = "📸 Snapshot current build";
};

$("#import-form").onsubmit = async (ev) => {
  ev.preventDefault();
  try {
    await api("/api/engines/import", {
      method: "POST",
      body: JSON.stringify({ name: $("#import-name").value, path: $("#import-path").value }),
    });
    toast("Imported");
    poll();
  } catch (e) { toast(e.message, true); }
};

// ── new match form ───────────────────────────────────────────────────────────

$("#set-sprt").onchange = () => $("#sprt-bounds").classList.toggle("hidden", !$("#set-sprt").checked);

function engineSpec(side) {
  const sel = document.getElementById(`engine-${side}`);
  const extra = document.getElementById(`extra-${side}`).value;
  if (sel.value === "stockfish") {
    return { type: "stockfish", elo: +document.getElementById(`elo-${side}`).value, extra_options: extra };
  }
  if (!sel.value) throw new Error("Pick two engines (snapshot a build first)");
  return { type: "snapshot", name: sel.value.slice(5), extra_options: extra };
}

$("#match-form").onsubmit = async (ev) => {
  ev.preventDefault();
  try {
    const body = {
      white: engineSpec("a"),
      black: engineSpec("b"),
      settings: {
        games: +$("#set-games").value,
        tc: $("#set-tc").value.trim(),
        concurrency: +$("#set-concurrency").value,
        hash: +$("#set-hash").value,
        openings: $("#set-openings").checked,
        draw_adjudication: $("#set-adjudication").checked,
        resign_adjudication: $("#set-adjudication").checked,
        sprt: $("#set-sprt").checked,
        sprt_elo0: +$("#set-elo0").value,
        sprt_elo1: +$("#set-elo1").value,
      },
    };
    const match = await api("/api/matches", { method: "POST", body: JSON.stringify(body) });
    toast("Match started");
    openDetail(match.id);
    poll();
  } catch (e) { toast(e.message, true); }
};

// ── matches list ─────────────────────────────────────────────────────────────

function scoreBar(r, total) {
  const n = Math.max(r.games_done, 1);
  const pct = (x) => (100 * x / n).toFixed(1) + "%";
  return `<div class="score-bar" title="+${r.wins} =${r.draws} -${r.losses}">
    <div class="w" style="width:${pct(r.wins)}"></div>
    <div class="d" style="width:${pct(r.draws)}"></div>
    <div class="l" style="width:${pct(r.losses)}"></div>
  </div>`;
}

function renderMatchList() {
  const div = $("#match-list");
  if (!state.matches.length) {
    div.innerHTML = `<p style="color:var(--dim)">No matches yet. Snapshot two builds and start one.</p>`;
    return;
  }
  div.innerHTML = state.matches.map((m) => {
    const r = m.results;
    const total = m.settings.games || "?";
    return `<div class="match-card" data-id="${m.id}">
      <div class="mc-top">
        <span class="mc-names">${esc(engineLabel(m.white))} <span class="vs">vs</span> ${esc(engineLabel(m.black))}</span>
        <span class="badge ${m.status}">${m.status}</span>
      </div>
      <div class="mc-stats">
        <span><b>+${r.wins} =${r.draws} -${r.losses}</b></span>
        <span>${r.games_done}/${total} games</span>
        <span>Elo <b>${eloStr(r)}</b></span>
        ${r.sprt_result ? `<span>SPRT <b>${r.sprt_result}</b></span>` : ""}
        <span>${esc(m.settings.tc || "")}</span>
      </div>
      ${scoreBar(r, total)}
    </div>`;
  }).join("");
  div.querySelectorAll(".match-card").forEach((card) => card.onclick = () => openDetail(card.dataset.id));
}

// ── match detail ─────────────────────────────────────────────────────────────

function openDetail(id) {
  currentMatchId = id;
  $("#matches-view").classList.add("hidden");
  $("#detail-view").classList.remove("hidden");
  refreshDetail();
}

$("#back-btn").onclick = () => {
  currentMatchId = null;
  $("#detail-view").classList.add("hidden");
  $("#matches-view").classList.remove("hidden");
};

$("#stop-btn").onclick = async () => {
  try { await api(`/api/matches/${currentMatchId}/stop`, { method: "POST" }); poll(); }
  catch (e) { toast(e.message, true); }
};

$("#delete-btn").onclick = async () => {
  if (!confirm("Delete this match and its games?")) return;
  try {
    await api(`/api/matches/${currentMatchId}`, { method: "DELETE" });
    $("#back-btn").click();
    poll();
  } catch (e) { toast(e.message, true); }
};

async function refreshDetail() {
  if (!currentMatchId) return;
  let m;
  try { m = await api(`/api/matches/${currentMatchId}`); }
  catch { return; }
  const r = m.results;

  $("#detail-title").textContent =
    `${engineLabel(m.white)} vs ${engineLabel(m.black)} — ${m.settings.tc}, ${m.settings.games} games`;
  $("#stop-btn").classList.toggle("hidden", m.status !== "running");
  $("#delete-btn").classList.toggle("hidden", m.status === "running");

  const stats = [
    ["Status", m.status],
    ["Score", `+${r.wins} =${r.draws} -${r.losses}`],
    ["Games", `${r.games_done}/${m.settings.games}`],
    ["Elo", eloStr(r), r.elo > 0 ? "pos" : r.elo < 0 ? "neg" : ""],
    ["LOS", r.los != null ? r.los + "%" : "—"],
  ];
  if (m.settings.sprt) {
    stats.push(["SPRT LLR", r.sprt_llr != null ? r.sprt_llr : "—"]);
    stats.push(["SPRT", r.sprt_result || "running", r.sprt_result === "PASSED" ? "pos" : r.sprt_result === "FAILED" ? "neg" : ""]);
  }
  $("#detail-stats").innerHTML = stats.map(([k, v, cls]) =>
    `<div class="stat"><div class="k">${k}</div><div class="v ${cls || ""}">${esc(v)}</div></div>`).join("");
  if (m.error) {
    $("#detail-stats").innerHTML += `<div class="stat"><div class="k">Error</div><div class="v neg">${esc(m.error)}</div></div>`;
  }
  $("#detail-bar").innerHTML = scoreBar(r, m.settings.games);

  // games table — engine A's perspective for win/loss colouring
  const tbody = $("#games-table tbody");
  tbody.innerHTML = (m.games || []).map((g) => {
    const aIsWhite = g.white === r.name1;
    const resClass = g.result === "1/2-1/2" ? "res-d"
      : (g.result === "1-0") === aIsWhite ? "res-w" : "res-l";
    return `<tr data-idx="${g.index}">
      <td>${g.index + 1}</td>
      <td>${esc(g.white)}</td>
      <td>${esc(g.black)}</td>
      <td class="${resClass}">${esc(g.result)}</td>
      <td>${esc(g.plies)}</td>
      <td>${esc(g.termination)}</td>
    </tr>`;
  }).join("");
  tbody.querySelectorAll("tr").forEach((tr) => tr.onclick = () => openReplay(+tr.dataset.idx));
}

// ── replay board ─────────────────────────────────────────────────────────────

const PIECES = {
  K: "♔", Q: "♕", R: "♖", B: "♗", N: "♘", P: "♙",
  k: "♚", q: "♛", r: "♜", b: "♝", n: "♞", p: "♟",
};

function renderBoard(fen) {
  const board = $("#board");
  const rows = fen.split(" ")[0].split("/");
  let html = "";
  for (let r = 0; r < 8; r++) {
    let f = 0;
    for (const ch of rows[r]) {
      if (/\d/.test(ch)) {
        for (let i = 0; i < +ch; i++, f++) html += sq(r, f, "");
      } else {
        html += sq(r, f, ch);
        f++;
      }
    }
  }
  board.innerHTML = html;

  function sq(r, f, piece) {
    const color = (r + f) % 2 === 0 ? "light" : "dark";
    const pieceCls = piece ? (piece === piece.toUpperCase() ? "wp" : "bp") : "";
    return `<div class="sq ${color} ${pieceCls}">${PIECES[piece] || ""}</div>`;
  }
}

async function openReplay(index) {
  try {
    const g = await api(`/api/matches/${currentMatchId}/games/${index}`);
    replay = { ...g, ply: g.fens.length - 1, index };
    $("#replay-title").textContent =
      `Game ${index + 1}: ${g.headers.White} vs ${g.headers.Black} — ${g.headers.Result}`;
    $("#move-list").innerHTML = g.sans.map((san, i) =>
      `<li data-ply="${i + 1}">${i % 2 === 0 ? `${Math.floor(i / 2) + 1}.` : ""} ${esc(san)}</li>`).join("");
    $("#move-list").querySelectorAll("li").forEach((li) => li.onclick = () => setPly(+li.dataset.ply));
    $("#replay-modal").classList.remove("hidden");
    setPly(0);
  } catch (e) { toast(e.message, true); }
}

function setPly(ply) {
  if (!replay) return;
  replay.ply = Math.max(0, Math.min(ply, replay.fens.length - 1));
  renderBoard(replay.fens[replay.ply]);
  $("#rp-pos").textContent = `${replay.ply} / ${replay.fens.length - 1}`;
  $("#move-list").querySelectorAll("li").forEach((li) =>
    li.classList.toggle("current", +li.dataset.ply === replay.ply));
  const cur = $("#move-list li.current");
  if (cur) cur.scrollIntoView({ block: "nearest" });
}

$("#rp-first").onclick = () => setPly(0);
$("#rp-prev").onclick = () => setPly(replay.ply - 1);
$("#rp-next").onclick = () => setPly(replay.ply + 1);
$("#rp-last").onclick = () => setPly(replay.fens.length - 1);
$("#replay-close").onclick = () => { $("#replay-modal").classList.add("hidden"); replay = null; };
$("#replay-modal").onclick = (ev) => {
  if (ev.target === $("#replay-modal")) $("#replay-close").click();
};

document.addEventListener("keydown", (ev) => {
  if (!replay || $("#replay-modal").classList.contains("hidden")) return;
  if (ev.key === "ArrowLeft") setPly(replay.ply - 1);
  else if (ev.key === "ArrowRight") setPly(replay.ply + 1);
  else if (ev.key === "Home") setPly(0);
  else if (ev.key === "End") setPly(replay.fens.length - 1);
  else if (ev.key === "Escape") $("#replay-close").click();
});

// ── boot ──
poll();
