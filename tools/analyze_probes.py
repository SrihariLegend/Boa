#!/usr/bin/env python3
"""
Probe JSONL analyzer — produces a self-contained interactive HTML report.

Usage:
    python3 tools/analyze_probes.py logs/boa-probe-*.jsonl
    python3 tools/analyze_probes.py logs/boa-probe-2026-07-03.jsonl -o report.html

Requires: duckdb, plotly, numpy, pandas
    pip install duckdb plotly numpy pandas
"""

import argparse
import json
import sys
from pathlib import Path

import duckdb
import numpy as np
import plotly.graph_objects as go
from plotly.subplots import make_subplots

# ── helpers ──────────────────────────────────────────────────────────────────


def F(col, sqltype):
    """Return (json_path, duckdb_type, alias) — alias is the json path key."""
    return (col, sqltype, col)


def sql_extract(typ, fields, where="", limit=""):
    """Build a duckdb query extracting typed columns from events of given typ."""
    parts = []
    for path, sqltype, alias in fields:
        parts.append(f"CAST(json_extract(json, '$.{path}') AS {sqltype}) AS {alias}")
    sql = f"SELECT {', '.join(parts)} FROM events WHERE typ='{typ}'"
    if where:
        sql += f" AND {where}"
    if limit:
        sql += f" LIMIT {limit}"
    return sql


def query(con, typ, fields, where="", limit=""):
    """Execute extract query, return DataFrame or None."""
    try:
        sql = sql_extract(typ, fields, where, limit)
        return con.execute(sql).df()
    except Exception as e:
        print(f"  WARNING: query failed for typ='{typ}': {e}")
        return None


def count(con, typ):
    try:
        return con.execute(f"SELECT count(*) FROM events WHERE typ='{typ}'").fetchone()[0]
    except Exception:
        return 0


# ── colour palette ──────────────────────────────────────────────────────────
PALETTE = {
    "bg": "#0d1117", "card": "#161b22", "border": "#30363d",
    "text": "#c9d1d9", "muted": "#8b949e", "accent": "#58a6ff",
    "green": "#3fb950", "red": "#f85149", "orange": "#d2991d",
    "purple": "#bc8cff", "teal": "#39d2c0",
}
COLOUR_SEQ = [PALETTE["accent"], PALETTE["green"], PALETTE["orange"],
              PALETTE["red"], PALETTE["purple"], PALETTE["teal"]]

PLOTLY_TMPL = dict(
    layout=dict(
        paper_bgcolor=PALETTE["bg"], plot_bgcolor=PALETTE["card"],
        font=dict(color=PALETTE["text"], size=12),
        title=dict(font=dict(size=16, color=PALETTE["text"])),
        xaxis=dict(gridcolor=PALETTE["border"], zerolinecolor=PALETTE["border"]),
        yaxis=dict(gridcolor=PALETTE["border"], zerolinecolor=PALETTE["border"]),
        legend=dict(font=dict(color=PALETTE["text"])),
        margin=dict(l=60, r=30, t=60, b=50), hovermode="closest",
    )
)


def style(fig, **kw):
    fig.update_layout(**PLOTLY_TMPL["layout"])
    fig.update_layout(colorway=COLOUR_SEQ, **kw)
    return fig


def plot_div(fig, height=None):
    if height:
        fig.update_layout(height=height)
    return f'<div class="plot-container">{fig.to_html(full_html=False, include_plotlyjs=False)}</div>'


# ── CSS ─────────────────────────────────────────────────────────────────────
CSS = """<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #0d1117; color: #c9d1d9; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif; padding: 2rem; }
  h1 { font-size: 2rem; margin-bottom: 0.25rem; color: #f0f6fc; }
  h2 { font-size: 1.4rem; margin: 2.5rem 0 1rem 0; padding-bottom: 0.4rem; border-bottom: 2px solid #30363d; color: #f0f6fc; }
  h3 { font-size: 1.1rem; margin: 1.5rem 0 0.5rem 0; color: #e6edf3; }
  .subtitle { color: #8b949e; font-size: 0.9rem; margin-bottom: 2rem; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(170px, 1fr)); gap: 0.6rem; margin-bottom: 1.5rem; }
  .card { background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 1rem; }
  .card .lbl { font-size: 0.7rem; color: #8b949e; text-transform: uppercase; letter-spacing: 0.05em; }
  .card .val { font-size: 1.3rem; font-weight: 600; color: #f0f6fc; margin-top: 0.2rem; }
  .card .u { font-size: 0.75rem; color: #8b949e; font-weight: 400; }
  .warn { background: #21262d; border-left: 3px solid #d2991d; padding: 0.75rem 1rem; border-radius: 4px; margin: 1rem 0; font-size: 0.85rem; color: #e6edf3; }
  .note { background: #1a2332; border-left: 3px solid #58a6ff; padding: 0.75rem 1rem; border-radius: 4px; margin: 0.5rem 0; font-size: 0.85rem; color: #c9d1d9; }
  .plot-container { margin: 1rem 0 2rem 0; }
  code { background: #21262d; padding: 2px 6px; border-radius: 3px; font-size: 0.85em; }
  .footer { margin-top: 3rem; padding-top: 1rem; border-top: 1px solid #30363d; color: #8b949e; font-size: 0.8rem; }
</style>"""


# ── data loading ────────────────────────────────────────────────────────────
def load_probe_data(file_patterns):
    con = duckdb.connect()
    paths = []
    import glob
    for pat in file_patterns:
        matches = sorted(glob.glob(pat))
        if matches:
            paths.extend(matches)
        else:
            print(f"WARNING: no files matching '{pat}'")
    if not paths:
        print("ERROR: no probe files found")
        sys.exit(1)

    # Parse meta from first file
    meta = None
    for p in paths:
        with open(p) as f:
            first = f.readline().strip()
            if first:
                try:
                    m = json.loads(first)
                    if m.get("typ") == "meta":
                        meta = m
                        break
                except json.JSONDecodeError:
                    continue

    field_legend = meta.get("fields", {}) if meta else {}

    file_list = ", ".join(f"'{p}'" for p in paths)
    con.execute(f"""
        CREATE OR REPLACE VIEW events AS
        SELECT
            json_extract_string(json, '$.typ') AS typ,
            json,
            filename
        FROM read_json_auto([{file_list}], format='newline_delimited',
                            ignore_errors=true)
        WHERE json_extract_string(json, '$.typ') NOT IN ('meta', 'xx')
    """)
    return con, field_legend, paths


# ═══════════════════════════════════════════════════════════════════════════════
# SECTIONS
# ═══════════════════════════════════════════════════════════════════════════════

def section_search_summary(con, paths):
    df = query(con, "ss", [
        F("td", "INTEGER"), F("ns", "BIGINT"), F("qs", "BIGINT"),
        F("tm", "BIGINT"), F("np", "BIGINT"), F("bm", "VARCHAR"),
        F("bs", "INTEGER"), F("sd", "INTEGER"),
        F("tt_p", "BIGINT"), F("tt_h", "BIGINT"), F("tt_c", "BIGINT"),
        F("bc", "BIGINT"), F("fc", "BIGINT"),
        F("nm_t", "BIGINT"), F("nm_c", "BIGINT"),
        F("rp", "BIGINT"), F("fp_a", "BIGINT"), F("fp_p", "BIGINT"),
        F("lm_a", "BIGINT"), F("lm_r", "BIGINT"), F("lm_rs", "BIGINT"),
        F("se_w", "BIGINT"), F("se_e", "BIGINT"), F("se_l", "BIGINT"),
        F("se_s", "BIGINT"),
        F("ii_t", "BIGINT"), F("ii_s", "BIGINT"),
        F("tb_h", "BIGINT"), F("dr", "BIGINT"),
    ])
    if df is None or len(df) == 0:
        return "<div class='warn'>No <code>ss</code> (SearchSummary) event. Run a search with probes enabled.</div>"

    r = df.iloc[0]
    td, ns, qs, tm_ms, np = int(r["td"]), int(r["ns"]), int(r["qs"]), int(r["tm"]), int(r["np"])
    bm, bs, sd = r["bm"], int(r["bs"]), int(r["sd"])
    tt_p, tt_h, tt_c = int(r["tt_p"]), int(r["tt_h"]), int(r["tt_c"])
    bc, fc = int(r["bc"]), int(r["fc"])
    nm_t, nm_c = int(r["nm_t"]), int(r["nm_c"])
    fp_a, fp_p = int(r["fp_a"]), int(r["fp_p"])
    lm_a, lm_r = int(r["lm_a"]), int(r["lm_r"])
    se_l, se_s = int(r["se_l"]), int(r["se_s"])
    ii_t, ii_s = int(r["ii_t"]), int(r["ii_s"])
    tb_h, dr = int(r["tb_h"]), int(r["dr"])
    rfp_c = int(r["rp"])

    tt_hit = (tt_h / tt_p * 100) if tt_p else 0
    tt_cut = (tt_c / tt_p * 100) if tt_p else 0
    ebf = (ns / td) ** (1.0 / td) if td and ns else 0
    ffp_r = (fp_p / fp_a * 100) if fp_a else 0
    nm_r = (nm_c / nm_t * 100) if nm_t else 0
    lm_avg = (lm_r / lm_a) if lm_a else 0
    fc_r = (fc / bc * 100) if bc else 0
    qs_pct = (qs / (ns + qs) * 100) if (ns + qs) else 0
    see_r = (se_s / se_l * 100) if se_l else 0
    iid_r = (ii_s / ii_t * 100) if ii_t else 0

    cards = [
        ("Depth", f"{td}", "plies"),
        ("Nodes", f"{ns:,}", ""),
        ("QSearch", f"{qs:,}", f"{qs_pct:.1f}%"),
        ("Time", f"{tm_ms:,}", "ms"),
        ("NPS", f"{np/1e6:.1f}", "Mnps"),
        ("Best Move", f"{bm}", ""),
        ("Score", f"{bs/100:.2f}", "pawns"),
        ("Sel Depth", f"{sd}", "plies"),
        ("EBF", f"{ebf:.2f}", ""),
        ("TT Hits", f"{tt_hit:.1f}%", f"{tt_h:,}"),
        ("TT Cutoffs", f"{tt_cut:.1f}%", f"{tt_c:,}"),
        ("β Cutoffs", f"{bc:,}", ""),
        ("1st-Move β", f"{fc_r:.1f}%", ""),
        ("FFP Prune", f"{ffp_r:.1f}%", f"{fp_p:,}/{fp_a:,}"),
        ("Null Move", f"{nm_r:.1f}%", f"{nm_c:,}/{nm_t:,}"),
        ("Avg LMR", f"{lm_avg:.1f}", "plies"),
        ("SEE Bad Srch", f"{see_r:.1f}%", f"{se_s:,}/{se_l:,}"),
        ("IID Success", f"{iid_r:.1f}%", f"{ii_s:,}/{ii_t:,}"),
        ("TB Hits", f"{tb_h:,}", ""),
        ("Dropped", f"{dr:,}", "probe events"),
    ]
    grid = "".join(
        f'<div class="card"><div class="lbl">{l}</div><div class="val">{v}<span class="u"> {" " + u if u else ""}</span></div></div>'
        for l, v, u in cards
    )
    return f'<div class="grid">{grid}</div>'


def section_iteration_timeline(con):
    df = query(con, "rt", [
        F("d", "INTEGER"), F("bs", "INTEGER"), F("it", "BIGINT"),
        F("ns", "BIGINT"), F("af", "INTEGER"), F("bm", "VARCHAR"),
        F("bc", "BOOLEAN"),
    ], where="", limit="")
    if df is None or len(df) < 2:
        return f"<div class='warn'>Need ≥2 <code>rt</code> events, got {count(con, 'rt')}.</div>"

    fig = make_subplots(rows=2, cols=2,
                        subplot_titles=("Time per Iteration", "Nodes per Iteration",
                                        "Best Score vs Depth", "Aspiration Fails"),
                        vertical_spacing=0.15, horizontal_spacing=0.12)
    fig.add_trace(go.Bar(x=df["d"], y=df["it"], marker_color=PALETTE["accent"],
                          hovertemplate="Depth %{x}<br>%{y} ms<extra></extra>"), row=1, col=1)
    fig.add_trace(go.Bar(x=df["d"], y=df["ns"], marker_color=PALETTE["green"],
                          hovertemplate="Depth %{x}<br>%{y:,} nodes<extra></extra>"), row=1, col=2)
    colours = [PALETTE["green"] if not ch else PALETTE["orange"] for ch in df["bc"]]
    fig.add_trace(go.Scatter(x=df["d"], y=df["bs"] / 100, mode="lines+markers",
                              marker=dict(color=colours, size=8),
                              line=dict(color=PALETTE["accent"]),
                              hovertemplate="Depth %{x}<br>%{y:.2f} pawns<extra></extra>"), row=2, col=1)
    fig.add_trace(go.Bar(x=df["d"], y=df["af"], marker_color=PALETTE["red"],
                          hovertemplate="Depth %{x}<br>%{y} fails<extra></extra>"), row=2, col=2)
    style(fig, height=650, showlegend=False, title="Iteration Timeline")
    changes = int(df["bc"].sum())
    msg = "stable — best move never changed" if changes == 0 else \
          f"best move changed {changes}× — moderate instability" if changes <= 3 else \
          f"best move changed {changes}× — significant search instability"
    return f'{plot_div(fig)}<div class="note">Search {msg}. Orange markers = best-move changed. Aspiration fails &gt;2/iter suggests too-narrow windows.</div>'


def section_pruning(con):
    html = ""
    fp_n, rp_n, lm_n, nm_n = count(con, "fp"), count(con, "rp"), count(con, "lm"), count(con, "nm")

    # ── FFP ──
    if fp_n > 50:
        df = query(con, "fp", [
            F("d", "INTEGER"), F("mi", "INTEGER"), F("hs", "INTEGER"),
            F("mg", "INTEGER"), F("rg", "INTEGER"), F("pr", "BOOLEAN"), F("cu", "BOOLEAN"),
        ])
        if df is not None and len(df) > 0:
            fig = make_subplots(rows=1, cols=2,
                                subplot_titles=("FFP Decision Boundary (d≤4)", "FFP Decision Boundary (d≥5)"),
                                horizontal_spacing=0.12)
            for i, mask in enumerate([df["d"] <= 4, df["d"] >= 5]):
                s = df[mask]
                kept, pruned = s[~s["pr"]], s[s["pr"]]
                fig.add_trace(go.Scatter(x=kept["mg"], y=kept["rg"], mode="markers",
                                          name=f"Kept", marker=dict(color=PALETTE["green"], size=3, opacity=0.35)), row=1, col=i + 1)
                fig.add_trace(go.Scatter(x=pruned["mg"], y=pruned["rg"], mode="markers",
                                          name=f"Pruned", marker=dict(color=PALETTE["red"], size=3, opacity=0.35)), row=1, col=i + 1)
            style(fig, height=420)
            html += f"<h3>Forward Futility Pruning</h3>{plot_div(fig)}"

            # FFP by depth
            dfd = df.groupby("d").agg(n=("d", "count"), prunes=("pr", "sum")).reset_index()
            fig2 = go.Figure()
            fig2.add_trace(go.Bar(x=dfd["d"], y=dfd["n"], name="Attempts", marker_color=PALETTE["accent"]))
            fig2.add_trace(go.Bar(x=dfd["d"], y=dfd["prunes"], name="Pruned", marker_color=PALETTE["red"]))
            style(fig2, height=320, barmode="overlay", title="FFP by Depth",
                  xaxis_title="Depth", yaxis_title="Count")
            html += plot_div(fig2)

            # Margin vs history
            fig3 = go.Figure()
            fig3.add_trace(go.Scatter(x=df["hs"], y=df["mg"], mode="markers",
                                       marker=dict(color=df["pr"].map({True: PALETTE["red"], False: PALETTE["green"]}),
                                                   size=2, opacity=0.3),
                                       hovertemplate="History: %{x}<br>Margin: %{y}<extra></extra>", showlegend=False))
            style(fig3, height=320, title="FFP Margin vs History Score",
                  xaxis_title="History Score (hs)", yaxis_title="Margin (mg)")
            html += plot_div(fig3)
        else:
            html += f"<div class='warn'>FFP: {fp_n} events but query failed.</div>"
    else:
        html += f"<div class='warn'>Only {fp_n} FFP events — skipping.</div>"

    # ── LMR ──
    if lm_n > 50:
        df = query(con, "lm", [
            F("d", "INTEGER"), F("mi", "INTEGER"), F("ar", "INTEGER"),
            F("br", "INTEGER"), F("ip", "BOOLEAN"), F("ki", "BOOLEAN"),
            F("co", "BOOLEAN"), F("hs", "INTEGER"), F("cu", "BOOLEAN"),
        ])
        if df is not None and len(df) > 0:
            # Heatmap
            hm = df[df["mi"] <= 40].groupby(["d", "mi"])["ar"].mean().reset_index()
            piv = hm.pivot_table(index="d", columns="mi", values="ar", aggfunc="mean")
            fig = go.Figure(data=go.Heatmap(
                z=piv.values, x=piv.columns, y=piv.index, colorscale="Viridis",
                hovertemplate="Depth: %{y}<br>Move#: %{x}<br>Avg Reduction: %{z:.1f}<extra></extra>",
                colorbar=dict(title="Avg R (plies)")))
            style(fig, height=450, title="LMR Reduction Heatmap (Depth × Move Index)",
                  xaxis_title="Move Index", yaxis_title="Depth")
            yaxis_dtick = max(1, int(len(piv.index) / 15))
            fig.update_yaxes(dtick=yaxis_dtick)
            html += f"<h3>Late Move Reductions</h3>{plot_div(fig)}"

            # LMR modifiers
            df["label"] = df.apply(lambda r: ("K" if r["ki"] else "C" if r["co"] else "Q"), axis=1)
            df["imp"] = df["ip"].map({True: "Improving", False: "Not Improving"})
            grp = df.groupby(["imp", "label"])["ar"].mean().reset_index()
            fig2 = go.Figure()
            for lbl in ["K", "C", "Q"]:
                s = grp[grp["label"] == lbl]
                fig2.add_trace(go.Bar(x=s["imp"], y=s["ar"], name={"K": "Killer", "C": "Counter", "Q": "Quiet"}[lbl],
                                       text=s["ar"].round(1), textposition="outside"))
            style(fig2, height=320, barmode="group", title="Avg LMR Reduction by Move Type & Improving",
                  xaxis_title="", yaxis_title="Avg Reduction (plies)")
            html += plot_div(fig2)

            # % reduced by depth
            df["reduced"] = df["ar"] > 0
            rd = df.groupby("d")["reduced"].mean().reset_index()
            rd["reduced"] *= 100
            fig3 = go.Figure()
            fig3.add_trace(go.Scatter(x=rd["d"], y=rd["reduced"], mode="lines+markers",
                                       line=dict(color=PALETTE["accent"], width=2), marker=dict(size=8)))
            style(fig3, height=300, title="% of Quiet Moves Reduced by Depth",
                  xaxis_title="Depth", yaxis_title="% Reduced", yaxis=dict(range=[0, 105]))
            html += plot_div(fig3)
        else:
            html += f"<div class='warn'>LMR: {lm_n} events but query failed.</div>"
    else:
        html += f"<div class='warn'>Only {lm_n} LMR events — skipping.</div>"

    # ── RFP ──
    if rp_n > 20:
        df = query(con, "rp", [
            F("d", "INTEGER"), F("se", "INTEGER"), F("b", "INTEGER"),
            F("mg", "INTEGER"), F("pr", "BOOLEAN"),
        ])
        if df is not None and len(df) > 0:
            dfd = df.groupby("d").agg(n=("d", "count"), prunes=("pr", "sum")).reset_index()
            fig = go.Figure()
            fig.add_trace(go.Bar(x=dfd["d"], y=dfd["n"], name="Attempts", marker_color=PALETTE["accent"]))
            fig.add_trace(go.Bar(x=dfd["d"], y=dfd["prunes"], name="Pruned", marker_color=PALETTE["red"]))
            style(fig, height=320, barmode="overlay", title="RFP by Depth", xaxis_title="Depth", yaxis_title="Count")
            html += f"<h3>Reverse Futility Pruning</h3>{plot_div(fig)}"
        else:
            html += f"<div class='warn'>RFP: {rp_n} events but query failed.</div>"

    # ── Null Move ──
    if nm_n > 20:
        df = query(con, "nm", [
            F("d", "INTEGER"), F("se", "INTEGER"), F("b", "INTEGER"),
            F("r", "INTEGER"), F("sc", "INTEGER"), F("pr", "BOOLEAN"),
        ])
        if df is not None and len(df) > 0:
            dfd = df.groupby("d").agg(n=("d", "count"), prunes=("pr", "sum"), avg_r=("r", "mean")).reset_index()
            fig = make_subplots(rows=1, cols=2,
                                subplot_titles=("Null Move Prune Rate by Depth", "Avg Reduction by Depth"))
            fig.add_trace(go.Bar(x=dfd["d"], y=dfd["n"], name="Attempts", marker_color=PALETTE["accent"]), row=1, col=1)
            fig.add_trace(go.Bar(x=dfd["d"], y=dfd["prunes"], name="Pruned", marker_color=PALETTE["red"]), row=1, col=1)
            fig.add_trace(go.Bar(x=dfd["d"], y=dfd["avg_r"], name="Avg R", marker_color=PALETTE["purple"]), row=1, col=2)
            style(fig, height=320, barmode="overlay", showlegend=True, title="Null Move Pruning")
            html += f"<h3>Null Move Pruning</h3>{plot_div(fig)}"
        else:
            html += f"<div class='warn'>Null Move: {nm_n} events but query failed.</div>"

    return html


def section_move_ordering(con):
    n = count(con, "mo")
    if n < 50:
        return f"<div class='warn'>Only {n} move ordering events — skipping.</div>"

    df = query(con, "mo", [
        F("p", "INTEGER"), F("mi", "INTEGER"), F("ph", "VARCHAR"),
        F("bf", "INTEGER"), F("kh", "INTEGER"), F("ch", "INTEGER"),
        F("ca", "INTEGER"), F("tt", "BOOLEAN"),
    ], where="mi <= 40")
    if df is None or len(df) == 0:
        return "<div class='warn'>Move ordering query failed.</div>"

    html = "<h3>Move Ordering Quality</h3>"

    first = df[df["mi"] == 0]
    vc = first["ph"].value_counts()
    phase_order = ["tt", "hash", "good_cap", "killer", "counter", "quiet", "bad_cap"]
    labels = [p for p in phase_order if p in vc.index]
    values = [int(vc.get(p, 0)) for p in labels]

    fig = go.Figure(data=go.Pie(labels=labels, values=values, hole=0.5,
                                 marker=dict(colors=COLOUR_SEQ[:len(labels)])))
    style(fig, height=380, title="Phase That Produces the First Move Picked")
    html += plot_div(fig)

    tt_pct = vc.get("tt", 0) / vc.sum() * 100 if vc.sum() else 0
    if tt_pct > 50:
        html += f'<div class="note">TT move is first pick {tt_pct:.0f}% of the time — move ordering is working.</div>'
    else:
        html += f'<div class="note">TT move first pick only {tt_pct:.0f}% — check history heuristic quality and killer aging.</div>'

    # Score distributions
    fig2 = make_subplots(rows=2, cols=2,
                         subplot_titles=("Butterfly Score", "Killer Score", "Counter Score", "Capture History Score"))
    for i, (col, name, clr) in enumerate([
        ("bf", "Butterfly", PALETTE["accent"]), ("kh", "Killer", PALETTE["green"]),
        ("ch", "Counter", PALETTE["orange"]), ("ca", "CapHist", PALETTE["purple"]),
    ]):
        r, c = i // 2 + 1, i % 2 + 1
        vals = df[col][df[col] != 0]
        if len(vals) > 0:
            fig2.add_trace(go.Histogram(x=vals, nbinsx=40, marker_color=clr, opacity=0.7), row=r, col=c)
    style(fig2, height=550, showlegend=False)
    html += plot_div(fig2)
    return html


def section_eval(con):
    n = count(con, "ev")
    if n < 100:
        return f"<div class='warn'>Only {n} eval events — need ≥100.</div>"

    mg_terms = {"Material": "ma_mg", "PST": "ps_mg", "Mobility": "mo_mg",
                "King Safety": "ks_mg", "Pawn": "pa_mg"}
    eg_terms = {"Material": "ma_eg", "PST": "ps_eg", "Mobility": "mo_eg",
                "King Safety": "ks_eg", "Pawn": "pa_eg"}

    all_cols = ["ph", "ws", "ss"]
    for d in [mg_terms, eg_terms]:
        all_cols.extend(d.values())
    field_specs = [(c, "INTEGER", c) for c in all_cols]

    df = query(con, "ev", field_specs)
    if df is None or len(df) == 0:
        return "<div class='warn'>Eval query failed.</div>"

    html = "<h3>Evaluation Breakdown</h3>"

    # Term std devs
    mg_s = [{"Term": nm, "StdDev": df[col].std(), "Mean": df[col].mean()} for nm, col in mg_terms.items()]
    eg_s = [{"Term": nm, "StdDev": df[col].std(), "Mean": df[col].mean()} for nm, col in eg_terms.items()]
    import pandas as pd
    dmg = pd.DataFrame(mg_s).sort_values("StdDev", ascending=False)
    deg = pd.DataFrame(eg_s).sort_values("StdDev", ascending=False)

    fig = make_subplots(rows=1, cols=2,
                        subplot_titles=("Midgame Term Std Dev", "Endgame Term Std Dev"),
                        horizontal_spacing=0.15)
    fig.add_trace(go.Bar(y=dmg["Term"], x=dmg["StdDev"], orientation="h",
                          marker_color=PALETTE["accent"]), row=1, col=1)
    fig.add_trace(go.Bar(y=deg["Term"], x=deg["StdDev"], orientation="h",
                          marker_color=PALETTE["green"]), row=1, col=2)
    style(fig, height=420, showlegend=False)
    html += plot_div(fig)

    dead = [r["Term"] for _, r in dmg.iterrows() if abs(r["StdDev"]) < 1 and abs(r["Mean"]) < 1] + \
           [r["Term"] for _, r in deg.iterrows() if abs(r["StdDev"]) < 1 and abs(r["Mean"]) < 1]
    if dead:
        html += f'<div class="warn">Dead eval terms (σ≈0): <code>{", ".join(dead)}</code>. Consider removing or retuning.</div>'

    # Score distributions
    fig2 = make_subplots(rows=1, cols=2, subplot_titles=("White Score", "Side-to-Move Score"))
    fig2.add_trace(go.Histogram(x=df["ws"], nbinsx=50, marker_color=PALETTE["accent"], opacity=0.7), row=1, col=1)
    fig2.add_trace(go.Histogram(x=df["ss"], nbinsx=50, marker_color=PALETTE["green"], opacity=0.7), row=1, col=2)
    style(fig2, height=320, showlegend=False)
    html += plot_div(fig2)

    # Phase distribution
    fig3 = go.Figure()
    fig3.add_trace(go.Histogram(x=df["ph"], nbinsx=30, marker_color=PALETTE["teal"], opacity=0.7))
    style(fig3, height=280, title="Phase Distribution (0=Pure MG, 24=Pure EG)",
          xaxis_title="Phase", yaxis_title="Count")
    html += plot_div(fig3)

    return html


def section_stability(con):
    n_rt, n_aw = count(con, "rt"), count(con, "aw")
    if n_rt < 2:
        return f"<div class='warn'>Only {n_rt} root events — need ≥2.</div>"

    html = "<h3>Search Stability</h3>"

    df = query(con, "rt", [
        F("d", "INTEGER"), F("bm", "VARCHAR"), F("bs", "INTEGER"),
        F("bc", "BOOLEAN"), F("pc", "VARCHAR"), F("it", "BIGINT"),
        F("ns", "BIGINT"), F("af", "INTEGER"),
    ])
    if df is None:
        return html

    # Best-move by iteration
    all_mv = list(dict.fromkeys(df["bm"]))
    mv_colours = {m: COLOUR_SEQ[i % len(COLOUR_SEQ)] for i, m in enumerate(all_mv)}
    fig = go.Figure()
    for _, row in df.iterrows():
        fig.add_trace(go.Scatter(
            x=[row["bs"] / 100], y=[f"Depth {int(row['d'])}"],
            mode="markers+text", text=row["bm"], textposition="middle right",
            textfont=dict(size=10),
            marker=dict(color=mv_colours.get(row["bm"], PALETTE["muted"]), size=14, symbol="square"),
            showlegend=False,
            hovertemplate="Depth %{y}<br>Best: %{text}<br>Score: %{x:.2f}<extra></extra>",
        ))
    style(fig, height=max(220, len(df) * 38), title="Best Move at Each Iteration",
          xaxis_title="Score (pawns)")
    html += plot_div(fig)

    if n_aw > 0:
        df2 = query(con, "aw", [
            F("d", "INTEGER"), F("dl", "INTEGER"), F("lo", "INTEGER"),
            F("hi", "INTEGER"), F("fh", "BOOLEAN"), F("fl", "BOOLEAN"),
            F("ex", "INTEGER"), F("rs", "INTEGER"),
        ])
        if df2 is not None and len(df2) > 0:
            fig2 = make_subplots(rows=1, cols=2,
                                 subplot_titles=("Aspiration Window Width", "Fails & Expansions"))
            fig2.add_trace(go.Scatter(x=df2["d"], y=df2["hi"] - df2["lo"],
                                       mode="lines+markers", line=dict(color=PALETTE["accent"], width=2),
                                       name="Width"), row=1, col=1)
            fhf = df2["fh"].astype(int) + df2["fl"].astype(int)
            fig2.add_trace(go.Bar(x=df2["d"], y=fhf, name="Fails", marker_color=PALETTE["orange"]), row=1, col=2)
            fig2.add_trace(go.Bar(x=df2["d"], y=df2["ex"], name="Expansions", marker_color=PALETTE["red"]), row=1, col=2)
            style(fig2, height=350, barmode="group")
            html += plot_div(fig2)
    return html


def section_tt(con):
    n_tt = count(con, "tt")
    if n_tt < 100:
        return f"<div class='warn'>Only {n_tt} TT probe events — skipping.</div>"

    html = "<h3>Transposition Table</h3>"
    df = query(con, "tt", [
        F("op", "VARCHAR"), F("h", "BOOLEAN"), F("et", "VARCHAR"),
        F("ed", "INTEGER"), F("ag", "INTEGER"), F("si", "INTEGER"), F("re", "BOOLEAN"),
    ], where="op='probe'")
    if df is None or len(df) == 0:
        return html

    hit_pct = df["h"].mean() * 100
    html += f'<div class="grid"><div class="card"><div class="lbl">TT Hit Rate</div><div class="val">{hit_pct:.1f}<span class="u">%</span></div></div></div>'

    # Hit by entry type
    grp = df.groupby("et").agg(n=("h", "count"), hit_rate=("h", "mean")).reset_index()
    grp["hit_rate"] *= 100
    fig = make_subplots(rows=1, cols=2, subplot_titles=("TT Hits by Entry Type", "Entry Depth: Hits vs Misses"))
    fig.add_trace(go.Bar(x=grp["et"], y=grp["n"], marker_color=PALETTE["accent"]), row=1, col=1)
    hits = df[df["h"]]["ed"]; misses = df[~df["h"]]["ed"]
    fig.add_trace(go.Histogram(x=hits, nbinsx=20, marker_color=PALETTE["green"], opacity=0.6, name="Hits"), row=1, col=2)
    fig.add_trace(go.Histogram(x=misses, nbinsx=20, marker_color=PALETTE["red"], opacity=0.6, name="Misses"), row=1, col=2)
    style(fig, height=380, barmode="overlay")
    html += plot_div(fig)

    # Age analysis
    ag = df.groupby("ag").agg(n=("h", "count"), hit_rate=("h", "mean")).reset_index()
    ag["hit_rate"] *= 100
    fig2 = make_subplots(rows=1, cols=2, subplot_titles=("Entry Age Distribution", "Hit Rate by Age"))
    fig2.add_trace(go.Bar(x=ag["ag"], y=ag["n"], marker_color=PALETTE["accent"]), row=1, col=1)
    fig2.add_trace(go.Scatter(x=ag["ag"], y=ag["hit_rate"], mode="lines+markers",
                               line=dict(color=PALETTE["green"], width=2)), row=1, col=2)
    style(fig2, height=320)
    html += plot_div(fig2)

    # TC cutoff depth sufficiency
    n_tc = count(con, "tc")
    if n_tc > 20:
        df2 = query(con, "tc", [F("df", "BOOLEAN")])
        if df2 is not None and len(df2) > 0:
            suff = df2["df"].mean() * 100
            html += f'<div class="note">TT cutoff entry depth is sufficient {suff:.1f}% of the time. {"Good." if suff > 80 else "Below 80% — TT replacement may be evicting deep entries too aggressively."}</div>'
    return html


def section_correction(con):
    n = count(con, "cr")
    if n < 20:
        return f"<div class='warn'>Only {n} correction events — skipping.</div>"

    html = "<h3>Correction History</h3>"
    df = query(con, "cr", [
        F("cv", "INTEGER"), F("re", "INTEGER"), F("ce", "INTEGER"),
        F("df", "INTEGER"), F("pc", "INTEGER"), F("np", "INTEGER"),
        F("cc", "INTEGER"), F("pl", "INTEGER"),
    ])
    if df is None or len(df) == 0:
        return html

    fig = make_subplots(rows=1, cols=2,
                        subplot_titles=("Correction vs Raw Eval", "Component Magnitudes (abs)"))
    fig.add_trace(go.Scatter(x=df["re"] / 100, y=df["cv"] / 512, mode="markers",
                              marker=dict(color=PALETTE["accent"], size=4, opacity=0.4),
                              hovertemplate="Raw: %{x:.2f}<br>Corr: %{y:.1f} cp<extra></extra>",
                              showlegend=False), row=1, col=1)
    for col, nm, clr in [("pc", "Pawn", PALETTE["accent"]), ("np", "Non-Pawn", PALETTE["green"]),
                           ("cc", "Cont", PALETTE["orange"])]:
        if col in df.columns:
            fig.add_trace(go.Box(y=df[col].abs(), name=nm, marker_color=clr), row=1, col=2)
    style(fig, height=380)
    html += plot_div(fig)

    avg_c = df["cv"].abs().mean() / 512
    html += f'<div class="note">Avg absolute correction: <b>{avg_c:.1f} cp</b>. ' + (
        "Large corrections — eval may have systematic biases." if avg_c > 50 else
        "Moderate corrections." if avg_c > 20 else "Small corrections — eval is well-calibrated.")
    html += '</div>'
    return html


def section_nodes(con):
    n = count(con, "sn")
    if n < 200:
        return f"<div class='warn'>Only {n} search node events — need ≥200.</div>"

    html = "<h3>Node Characteristics</h3>"
    df = query(con, "sn", [
        F("d", "INTEGER"), F("p", "INTEGER"), F("pv", "BOOLEAN"),
        F("cu", "BOOLEAN"), F("ck", "BOOLEAN"), F("im", "BOOLEAN"),
        F("fc", "BOOLEAN"), F("nm", "INTEGER"),
    ], where="d >= 1")
    if df is None or len(df) == 0:
        return html

    grp = df.groupby("d").agg(
        n=("d", "count"), cut_pct=("cu", "mean"), pv_pct=("pv", "mean"),
        check_pct=("ck", "mean"), improving_pct=("im", "mean"),
        fc_pct=("fc", "mean"), avg_moves=("nm", "mean"),
    ).reset_index()
    for c in ["cut_pct", "pv_pct", "check_pct", "improving_pct", "fc_pct"]:
        grp[c] *= 100

    fig = make_subplots(rows=2, cols=2,
                        subplot_titles=("Cut-Node Rate", "1st-Move Cutoff Rate",
                                        "Improving Rate", "Avg Moves Searched"))
    fig.add_trace(go.Scatter(x=grp["d"], y=grp["cut_pct"], mode="lines+markers",
                              line=dict(color=PALETTE["red"], width=2)), row=1, col=1)
    fig.add_trace(go.Scatter(x=grp["d"], y=grp["fc_pct"], mode="lines+markers",
                              line=dict(color=PALETTE["green"], width=2)), row=1, col=2)
    fig.add_trace(go.Scatter(x=grp["d"], y=grp["improving_pct"], mode="lines+markers",
                              line=dict(color=PALETTE["accent"], width=2)), row=2, col=1)
    fig.add_trace(go.Scatter(x=grp["d"], y=grp["avg_moves"], mode="lines+markers",
                              line=dict(color=PALETTE["orange"], width=2)), row=2, col=2)
    style(fig, height=550, showlegend=False)
    html += plot_div(fig)
    return html


def section_see(con):
    n = count(con, "se")
    if n < 50:
        return f"<div class='warn'>Only {n} SEE events — skipping.</div>"

    html = "<h3>Static Exchange Evaluation</h3>"
    df = query(con, "se", [
        F("vl", "INTEGER"), F("cv", "INTEGER"), F("th", "INTEGER"),
        F("pr", "BOOLEAN"), F("sr", "BOOLEAN"), F("px", "BOOLEAN"),
    ])
    if df is None or len(df) == 0:
        return html

    fig = make_subplots(rows=1, cols=2, subplot_titles=("SEE Value Distribution", "Captured Value vs Threshold"))
    fig.add_trace(go.Histogram(x=df[df["pr"]]["vl"], nbinsx=30, marker_color=PALETTE["red"], opacity=0.6,
                                name="Pruned"), row=1, col=1)
    fig.add_trace(go.Histogram(x=df[~df["pr"]]["vl"], nbinsx=30, marker_color=PALETTE["green"], opacity=0.6,
                                name="Searched"), row=1, col=1)
    fig.add_trace(go.Scatter(x=df["cv"], y=df["th"], mode="markers",
                              marker=dict(color=df["pr"].map({True: PALETTE["red"], False: PALETTE["green"]}),
                                          size=2, opacity=0.3), showlegend=False), row=1, col=2)
    style(fig, height=380, barmode="overlay")
    html += plot_div(fig)

    sr_pct = df["sr"].mean() * 100
    html += f'<div class="note">{sr_pct:.1f}% of bad-SEE captures searched despite negative SEE. Too high = wasted nodes; too low (≈0%) may miss tactics.</div>'
    return html


def section_qsearch(con):
    n = count(con, "qs")
    if n < 50:
        return f"<div class='warn'>Only {n} qsearch events — skipping.</div>"

    html = "<h3>Quiescence Search</h3>"
    df = query(con, "qs", [
        F("p", "INTEGER"), F("sp", "INTEGER"), F("sc", "INTEGER"),
        F("nc", "INTEGER"), F("dp", "INTEGER"), F("se", "INTEGER"),
        F("ck", "BOOLEAN"), F("fc", "BOOLEAN"),
    ])
    if df is None or len(df) == 0:
        return html

    fig = make_subplots(rows=1, cols=2,
                        subplot_titles=("Captures Searched per QNode", "Stand-Pat vs Final Score"))
    fig.add_trace(go.Histogram(x=df["nc"], nbinsx=25, marker_color=PALETTE["accent"], opacity=0.7), row=1, col=1)
    fig.add_trace(go.Scatter(x=df["sp"] / 100, y=df["sc"] / 100, mode="markers",
                              marker=dict(color=PALETTE["teal"], size=2, opacity=0.3),
                              hovertemplate="Stand Pat: %{x:.2f}<br>Final: %{y:.2f}<extra></extra>"),
                  row=1, col=2)
    style(fig, height=380, showlegend=False)
    html += plot_div(fig)

    fc_pct = df["fc"].mean() * 100
    avg_caps = df["nc"].mean()
    html += f'<div class="note">QSearch futility cutoff rate: <b>{fc_pct:.1f}%</b>. Avg captures/QNode: <b>{avg_caps:.1f}</b>. '
    if avg_caps > 5:
        html += '⚠ QSearch is exploding — tighten delta/futility margins.</div>'
    else:
        html += 'Healthy.</div>'
    return html


def section_continuation_history(con):
    n = count(con, "ch")
    if n < 5:
        return ""
    html = "<h3>Continuation History Health</h3>"
    df = query(con, "ch", [
        F("tb", "VARCHAR"), F("hr", "DOUBLE"), F("as", "DOUBLE"),
        F("mx", "INTEGER"), F("uf", "BIGINT"),
    ])
    if df is None or len(df) == 0:
        return html

    fig = make_subplots(rows=1, cols=2, subplot_titles=("Hit Rate by Table", "Avg Score by Table"))
    fig.add_trace(go.Bar(x=df["tb"], y=df["hr"] * 100, marker_color=PALETTE["accent"]), row=1, col=1)
    fig.add_trace(go.Bar(x=df["tb"], y=df["as"], marker_color=PALETTE["green"]), row=1, col=2)
    style(fig, height=320, showlegend=False)
    html += plot_div(fig)
    return html


def section_inventory(con, paths):
    types = ["cf", "b", "mg", "ev", "sn", "ss", "tt", "tc", "fp", "rp", "lm",
             "nm", "se", "qs", "aw", "ii", "mo", "ht", "rt", "tm", "tz", "dd",
             "md", "ch", "cr"]
    names = {
        "cf": "Config", "b": "Board", "mg": "Movegen", "ev": "Eval",
        "sn": "Search Node", "ss": "Search Summary", "tt": "TT Probe",
        "tc": "TT Cutoff", "fp": "FFP", "rp": "RFP", "lm": "LMR",
        "nm": "Null Move", "se": "SEE", "qs": "Quiescence", "aw": "Aspiration",
        "ii": "IID", "mo": "Move Ordering", "ht": "History Table", "rt": "Root",
        "tm": "Time Mgmt", "tz": "Syzygy", "dd": "Draw Detection",
        "md": "Mate Distance", "ch": "Cont. History", "cr": "Correction History",
    }
    counts = {t: count(con, t) for t in types}
    present = [(t, counts[t]) for t in types if counts[t] > 0]
    missing = [t for t in types if counts[t] == 0]
    total = sum(counts.values())

    rows = "".join(
        f'<tr><td style="padding:3px 10px"><code>{t}</code></td>'
        f'<td style="padding:3px 10px">{names.get(t, "?")}</td>'
        f'<td style="padding:3px 10px;text-align:right">{c:,}</td></tr>'
        for t, c in present
    )
    html = f"""
    <h3>Event Inventory</h3>
    <p style="color:#8b949e;font-size:0.85em">{len(paths)} file(s), {total:,} total events (excl. meta)</p>
    <table style="font-size:0.85em;border-collapse:collapse">
      <tr style="color:#8b949e;text-align:left"><th style="padding:3px 10px">Code</th><th style="padding:3px 10px">Name</th><th style="padding:3px 10px;text-align:right">Count</th></tr>
      {rows}
    </table>
    """
    if missing:
        html += f'<p style="color:#8b949e;font-size:0.8em;margin-top:0.5em">Not emitted: {", ".join(f"<code>{t}</code>" for t in missing)}</p>'
    return html


# ═══════════════════════════════════════════════════════════════════════════════
# TEXT / MARKDOWN REPORT (for LLM ingestion)
# ═══════════════════════════════════════════════════════════════════════════════

def _md_table(df, cols=None):
    """Format a pandas DataFrame as a markdown table."""
    if cols:
        df = df[cols]
    lines = ["| " + " | ".join(str(c) for c in df.columns) + " |",
             "|" + "|".join("---" for _ in df.columns) + "|"]
    for _, row in df.iterrows():
        vals = []
        for c in df.columns:
            v = row[c]
            if isinstance(v, float):
                vals.append(f"{v:.2f}")
            elif isinstance(v, int) and abs(v) > 9999:
                vals.append(f"{v:,}")
            else:
                vals.append(str(v))
        lines.append("| " + " | ".join(vals) + " |")
    return "\n".join(lines)


def text_report(con, paths):
    """Produce a markdown report suitable for LLM ingestion."""
    md = []
    def h1(t): md.append(f"\n# {t}\n")
    def h2(t): md.append(f"\n## {t}\n")
    def h3(t): md.append(f"\n### {t}\n")
    def p(t): md.append(f"{t}\n")
    def note(t): md.append(f"> **Note:** {t}\n")
    def warn(t): md.append(f"> ⚠ **Warning:** {t}\n")

    fnames = ", ".join(Path(p).name for p in paths[:5])
    md.append(f"# Boa Probe Analysis — Text Report")
    md.append(f"*Generated from {fnames}*\n")

    # ── 1. Search Overview ──
    h1("1. Search Overview")
    df = query(con, "ss", [
        F("td", "INTEGER"), F("ns", "BIGINT"), F("qs", "BIGINT"),
        F("tm", "BIGINT"), F("np", "BIGINT"), F("bm", "VARCHAR"),
        F("bs", "INTEGER"), F("sd", "INTEGER"),
        F("tt_p", "BIGINT"), F("tt_h", "BIGINT"), F("tt_c", "BIGINT"),
        F("bc", "BIGINT"), F("fc", "BIGINT"),
        F("nm_t", "BIGINT"), F("nm_c", "BIGINT"),
        F("fp_a", "BIGINT"), F("fp_p", "BIGINT"),
        F("lm_a", "BIGINT"), F("lm_r", "BIGINT"),
        F("se_l", "BIGINT"), F("se_s", "BIGINT"),
        F("ii_t", "BIGINT"), F("ii_s", "BIGINT"),
        F("tb_h", "BIGINT"), F("dr", "BIGINT"), F("rp", "BIGINT"),
    ])
    if df is not None and len(df) > 0:
        r = df.iloc[0]
        td, ns, qs, tm_ms, np_val = int(r["td"]), int(r["ns"]), int(r["qs"]), int(r["tm"]), int(r["np"])
        bm, bs, sd = r["bm"], int(r["bs"]), int(r["sd"])
        tt_p, tt_h, tt_c = int(r["tt_p"]), int(r["tt_h"]), int(r["tt_c"])
        bc, fc = int(r["bc"]), int(r["fc"])
        nm_t, nm_c = int(r["nm_t"]), int(r["nm_c"])
        fp_a, fp_p = int(r["fp_a"]), int(r["fp_p"])
        lm_a, lm_r = int(r["lm_a"]), int(r["lm_r"])
        se_l, se_s = int(r["se_l"]), int(r["se_s"])
        ii_t, ii_s = int(r["ii_t"]), int(r["ii_s"])
        tb_h, dr = int(r["tb_h"]), int(r["dr"])
        rfp_c = int(r["rp"])

        def pct(a, b): return f"{a/b*100:.1f}%" if b else "N/A"
        def div(a, b, dec=1): return f"{a/b:.{dec}f}" if b else "N/A"

        rows = [
            ("Depth completed", td),
            ("Total nodes (main search)", f"{ns:,}"),
            ("QSearch nodes", f"{qs:,} ({pct(qs, ns+qs)} of total)"),
            ("Time", f"{tm_ms:,} ms"),
            ("Nodes per second", f"{np_val/1e6:.1f} Mnps"),
            ("Best move", bm),
            ("Best score", f"{bs/100:.2f} pawns"),
            ("Selective depth", sd),
            ("Effective branching factor", f"{(ns/td)**(1.0/td):.2f}" if td and ns else "N/A"),
            ("TT hit rate", f"{pct(tt_h, tt_p)} ({tt_h:,} hits / {tt_p:,} probes)"),
            ("TT cutoff rate", f"{pct(tt_c, tt_p)}"),
            ("Beta cutoffs", f"{bc:,}"),
            ("First-move beta cutoff rate", f"{pct(fc, bc)}"),
            ("FFP prune rate", f"{pct(fp_p, fp_a)} ({fp_p:,} / {fp_a:,})"),
            ("RFP cutoffs", f"{rfp_c:,}"),
            ("Null-move prune rate", f"{pct(nm_c, nm_t)} ({nm_c:,} / {nm_t:,})"),
            ("Avg LMR reduction", f"{div(lm_r, lm_a)} plies"),
            ("SEE bad-caps searched", f"{pct(se_s, se_l)} ({se_s:,} / {se_l:,})"),
            ("IID success rate", f"{pct(ii_s, ii_t)}"),
            ("TB hits", f"{tb_h:,}"),
            ("Dropped probe events", f"{dr:,}"),
        ]
        for metric, value in rows:
            md.append(f"- **{metric}:** {value}")
        p("")
    else:
        warn("No search summary found. Run a search with probes enabled (`--features probes`).")

    # ── 2. Iteration Timeline ──
    h1("2. Iteration Timeline")
    df = query(con, "rt", [
        F("d", "INTEGER"), F("bs", "INTEGER"), F("it", "BIGINT"),
        F("ns", "BIGINT"), F("af", "INTEGER"), F("bm", "VARCHAR"), F("bc", "BOOLEAN"),
    ])
    if df is not None and len(df) > 0:
        df["score_pawns"] = df["bs"] / 100
        df["changed"] = df["bc"].map({True: "YES", False: ""})
        p(_md_table(df[["d", "bm", "score_pawns", "changed", "it", "ns", "af"]],
                    ["d", "bm", "score_pawns", "changed", "it", "ns", "af"]))
        changes = int(df["bc"].sum())
        if changes == 0:
            note("Best move never changed — search is stable.")
        elif changes <= 3:
            note(f"Best move changed {changes} time(s) — moderate instability.")
        else:
            note(f"Best move changed {changes} times — significant search instability. Check aspiration window widths and pruning aggressiveness at failing depths.")
    else:
        warn(f"Need ≥2 root (rt) events, got {count(con, 'rt')}.")

    # ── 3. Pruning ──
    h1("3. Pruning System")

    h2("Forward Futility Pruning (FFP)")
    fp_n = count(con, "fp")
    if fp_n > 50:
        df = query(con, "fp", [
            F("d", "INTEGER"), F("mi", "INTEGER"), F("hs", "INTEGER"),
            F("mg", "INTEGER"), F("rg", "INTEGER"), F("pr", "BOOLEAN"),
        ])
        if df is not None and len(df) > 0:
            grp = df.groupby("d").agg(attempts=("d", "count"), prunes=("pr", "sum"),
                                       prune_pct=("pr", "mean")).reset_index()
            grp["prune_pct"] *= 100
            p(_md_table(grp, ["d", "attempts", "prunes", "prune_pct"]))

            # Overlap analysis: how many pruned moves had margin > required_gain?
            overlap = ((df["pr"]) & (df["mg"] > df["rg"])).sum()
            false_pos = overlap / df["pr"].sum() * 100 if df["pr"].sum() > 0 else 0
            p(f"**False-positive prune rate:** {false_pos:.1f}% of pruned moves had estimated gain > required gain.")
            if false_pos > 10:
                warn("High FP rate — FFP is pruning moves that the margin formula thinks are good. Increase FFP_BUFFER or tune margin weights.")

            # Deep pruning check
            deep = df[df["d"] >= 6]
            if len(deep) > 0:
                deep_rate = deep["pr"].mean() * 100
                p(f"**Deep FFP rate (d≥6):** {deep_rate:.1f}% — {'aggressive' if deep_rate > 70 else 'moderate' if deep_rate > 40 else 'conservative'}.")
        else:
            warn(f"FFP query returned no data.")
    else:
        warn(f"Only {fp_n} FFP events.")

    h2("Late Move Reductions (LMR)")
    lm_n = count(con, "lm")
    if lm_n > 50:
        df = query(con, "lm", [
            F("d", "INTEGER"), F("mi", "INTEGER"), F("ar", "INTEGER"),
            F("br", "INTEGER"), F("ip", "BOOLEAN"), F("ki", "BOOLEAN"),
            F("co", "BOOLEAN"), F("hs", "INTEGER"),
        ])
        if df is not None and len(df) > 0:
            grp = df.groupby("d").agg(
                avg_base=("br", "mean"), avg_actual=("ar", "mean"),
                reduced_pct=("ar", lambda x: (x > 0).mean() * 100), n=("d", "count")
            ).reset_index()
            p(_md_table(grp, ["d", "n", "avg_base", "avg_actual", "reduced_pct"]))

            # Modifier impact
            for label, mask, name in [
                ("ki", df["ki"], "Killer moves"),
                ("co", df["co"], "Counter moves"),
                ("ip", df["ip"], "Improving position"),
            ]:
                if mask.sum() > 10:
                    r_yes = df[mask]["ar"].mean()
                    r_no = df[~mask]["ar"].mean()
                    p(f"- **{name}:** avg reduction {r_yes:.1f} vs {r_no:.1f} without — delta={r_yes - r_no:+.1f}")
        else:
            warn(f"LMR query returned no data.")
    else:
        warn(f"Only {lm_n} LMR events.")

    h2("Reverse Futility Pruning (RFP)")
    rp_n = count(con, "rp")
    if rp_n > 20:
        df = query(con, "rp", [F("d", "INTEGER"), F("se", "INTEGER"), F("b", "INTEGER"),
                                F("mg", "INTEGER"), F("pr", "BOOLEAN")])
        if df is not None and len(df) > 0:
            grp = df.groupby("d").agg(attempts=("d", "count"), prunes=("pr", "sum"),
                                       prune_pct=("pr", "mean")).reset_index()
            grp["prune_pct"] *= 100
            p(_md_table(grp, ["d", "attempts", "prunes", "prune_pct"]))

            # avg margin by prune decision
            margin_pruned = df[df["pr"]]["mg"].mean()
            margin_kept = df[~df["pr"]]["mg"].mean()
            p(f"Avg margin when pruned: {margin_pruned:.0f} cp, when kept: {margin_kept:.0f} cp.")
        else:
            warn(f"RFP query returned no data.")
    else:
        warn(f"Only {rp_n} RFP events.")

    h2("Null Move Pruning")
    nm_n = count(con, "nm")
    if nm_n > 20:
        df = query(con, "nm", [F("d", "INTEGER"), F("se", "INTEGER"), F("b", "INTEGER"),
                                F("r", "INTEGER"), F("sc", "INTEGER"), F("pr", "BOOLEAN")])
        if df is not None and len(df) > 0:
            grp = df.groupby("d").agg(attempts=("d", "count"), prunes=("pr", "sum"),
                                       prune_pct=("pr", "mean"), avg_R=("r", "mean")).reset_index()
            grp["prune_pct"] *= 100
            p(_md_table(grp, ["d", "attempts", "prunes", "prune_pct", "avg_R"]))
        else:
            warn(f"Null move query returned no data.")

    # ── 4. Move Ordering ──
    h1("4. Move Ordering")
    n = count(con, "mo")
    if n < 50:
        warn(f"Only {n} move ordering events — cannot analyze. The `mo` probe is defined but not yet wired to the move picker. Add `probe!()` calls in `src/search/move_ordering.rs`.")
    else:
        df = query(con, "mo", [F("ph", "VARCHAR")], where="mi=0")
        if df is not None and len(df) > 0:
            vc = df["ph"].value_counts()
            p("**Phase that produced the first-picked move:**")
            for ph, cnt in vc.items():
                p(f"- {ph}: {cnt} ({cnt/vc.sum()*100:.1f}%)")
            tt_pct = vc.get("tt", 0) / vc.sum() * 100
            if tt_pct > 50:
                note(f"TT move first pick {tt_pct:.0f}% — move ordering is working well.")
            else:
                warn(f"TT move first pick only {tt_pct:.0f}% — check history heuristic quality.")

    # ── 5. Eval Breakdown ──
    h1("5. Evaluation Breakdown")
    n = count(con, "ev")
    if n >= 100:
        mg_terms = {"Material": "ma_mg", "PST": "ps_mg", "Mobility": "mo_mg",
                    "King Safety": "ks_mg", "Pawn": "pa_mg"}
        eg_terms = {"Material": "ma_eg", "PST": "ps_eg", "Mobility": "mo_eg",
                    "King Safety": "ks_eg", "Pawn": "pa_eg"}

        all_fields = [(v, "INTEGER", v) for v in list(mg_terms.values()) + list(eg_terms.values())]
        df = query(con, "ev", all_fields)
        if df is not None and len(df) > 0:
            h2("Midgame Term Contribution (higher stddev = more impactful)")
            rows = []
            for nm, col in mg_terms.items():
                if col in df.columns:
                    rows.append({"Term": nm, "Mean": df[col].mean(), "StdDev": df[col].std(),
                                 "Min": df[col].min(), "Max": df[col].max(), "NonZero%": (df[col] != 0).mean() * 100})
            import pandas as pd
            dmg = pd.DataFrame(rows).sort_values("StdDev", ascending=False)
            p(_md_table(dmg, ["Term", "Mean", "StdDev", "Min", "Max", "NonZero%"]))

            h2("Endgame Term Contribution")
            rows2 = []
            for nm, col in eg_terms.items():
                if col in df.columns:
                    rows2.append({"Term": nm, "Mean": df[col].mean(), "StdDev": df[col].std(),
                                  "Min": df[col].min(), "Max": df[col].max(), "NonZero%": (df[col] != 0).mean() * 100})
            deg = pd.DataFrame(rows2).sort_values("StdDev", ascending=False)
            p(_md_table(deg, ["Term", "Mean", "StdDev", "Min", "Max", "NonZero%"]))

            dead = [r["Term"] for _, r in dmg.iterrows() if abs(r["StdDev"]) < 1 and abs(r["Mean"]) < 1] + \
                   [r["Term"] for _, r in deg.iterrows() if abs(r["StdDev"]) < 1 and abs(r["Mean"]) < 1]
            if dead:
                warn(f"Dead eval terms (σ≈0, μ≈0): {', '.join(dead)}. Remove or retune their weights.")
        else:
            warn("Eval query returned no data.")
    else:
        warn(f"Only {n} eval events.")

    # ── 6. Search Stability ──
    h1("6. Search Stability")
    df = query(con, "rt", [F("d", "INTEGER"), F("bm", "VARCHAR"), F("bs", "INTEGER"),
                            F("bc", "BOOLEAN"), F("pc", "VARCHAR"), F("af", "INTEGER")])
    if df is not None and len(df) > 0:
        df["score_pawns"] = df["bs"] / 100
        p(_md_table(df[["d", "bm", "score_pawns", "bc", "af", "pc"]],
                    ["d", "bm", "score_pawns", "bc", "af", "pc"]))
        moves = list(df["bm"].unique())
        if len(moves) > 3:
            warn(f"Engine considered {len(moves)} different best moves across {len(df)} iterations — search may be unstable.")
        elif len(moves) > 1:
            note(f"Best move settled after exploring {len(moves)} candidates — normal convergence.")
        else:
            note("Single best move throughout — very stable search.")
    else:
        warn(f"Only {count(con, 'rt')} root events.")

    df_aw = query(con, "aw", [F("d", "INTEGER"), F("fh", "BOOLEAN"), F("fl", "BOOLEAN"),
                               F("ex", "INTEGER")])
    if df_aw is not None and len(df_aw) > 0:
        total_fails = int(df_aw["fh"].sum() + df_aw["fl"].sum())
        total_exp = int(df_aw["ex"].sum())
        p(f"Aspiration fails: {total_fails} (high + low). Total expansions: {total_exp}.")
        if total_fails > len(df_aw) * 2:
            warn("High aspiration fail rate — initial window may be too narrow.")
    else:
        p("No aspiration window events — `aw` probe not yet wired in search code.")

    # ── 7. TT ──
    h1("7. Transposition Table")
    df = query(con, "tt", [
        F("op", "VARCHAR"), F("h", "BOOLEAN"), F("et", "VARCHAR"),
        F("ed", "INTEGER"), F("ag", "INTEGER"), F("re", "BOOLEAN"),
    ], where="op='probe'")
    if df is not None and len(df) > 0:
        hit_rate = df["h"].mean() * 100
        p(f"**TT hit rate:** {hit_rate:.1f}% ({int(df['h'].sum()):,} / {len(df):,})")
        p(f"**Replacement rate:** {df['re'].mean()*100:.1f}% of probes replaced an existing entry")

        grp = df.groupby("et").agg(n=("h", "count"), hit_rate=("h", "mean")).reset_index()
        grp["hit_rate"] *= 100
        p(_md_table(grp, ["et", "n", "hit_rate"]))

        ag = df.groupby("ag").agg(n=("h", "count"), hit_rate=("h", "mean")).reset_index()
        ag["hit_rate"] *= 100
        p("**Hit rate by entry age (higher age = older entry):**")
        p(_md_table(ag, ["ag", "n", "hit_rate"]))

        if hit_rate < 60:
            warn("TT hit rate below 60% — table may be too small for the search depth, or replacement strategy is too aggressive.")
        elif hit_rate > 85:
            note("TT hit rate is excellent (>85%).")
    else:
        warn(f"Only {count(con, 'tt')} TT probe events.")

    # TT cutoff depth sufficiency
    df2 = query(con, "tc", [F("d", "INTEGER"), F("ed", "INTEGER"), F("df", "BOOLEAN"), F("et", "VARCHAR")])
    if df2 is not None and len(df2) > 0:
        suff = df2["df"].mean() * 100
        p(f"**TT cutoff depth sufficient:** {suff:.1f}% of the time")
        if suff < 80:
            warn("Below 80% — TT replacement is evicting deep entries before they can be used.")

    # ── 8. Node Characteristics ──
    h1("8. Node Characteristics")
    n = count(con, "sn")
    if n < 200:
        warn(f"Only {n} search node events — `sn` probe not wired. Add `sample_probe!()` in `alpha_beta.rs`.")
    else:
        df = query(con, "sn", [
            F("d", "INTEGER"), F("cu", "BOOLEAN"), F("pv", "BOOLEAN"),
            F("ck", "BOOLEAN"), F("im", "BOOLEAN"), F("fc", "BOOLEAN"),
            F("nm", "INTEGER"),
        ], where="d >= 1")
        if df is not None and len(df) > 0:
            grp = df.groupby("d").agg(
                n=("d", "count"), cut_pct=("cu", "mean"), pv_pct=("pv", "mean"),
                check_pct=("ck", "mean"), improving_pct=("im", "mean"),
                fc_pct=("fc", "mean"), avg_moves=("nm", "mean"),
            ).reset_index()
            for c in ["cut_pct", "pv_pct", "check_pct", "improving_pct", "fc_pct"]:
                grp[c] *= 100
            p(_md_table(grp, ["d", "n", "cut_pct", "fc_pct", "improving_pct", "avg_moves"]))

    # ── 9. SEE ──
    h1("9. Static Exchange Evaluation")
    n = count(con, "se")
    if n >= 50:
        df = query(con, "se", [
            F("vl", "INTEGER"), F("cv", "INTEGER"), F("th", "INTEGER"),
            F("pr", "BOOLEAN"), F("sr", "BOOLEAN"),
        ])
        if df is not None and len(df) > 0:
            p(f"**SEE prunes:** {df['pr'].mean()*100:.1f}% of captures pruned")
            p(f"**Bad-SEE captures searched:** {df['sr'].mean()*100:.1f}% (captures with negative SEE that were searched anyway)")
            p(f"**SEE value range:** [{df['vl'].min()}, {df['vl'].max()}], mean={df['vl'].mean():.1f}")
            # avg vl by prune decision
            p(f"Avg SEE value when pruned: {df[df['pr']]['vl'].mean():.1f}, when searched: {df[~df['pr']]['vl'].mean():.1f}")
        else:
            warn("SEE query failed.")
    else:
        warn(f"Only {n} SEE events.")

    # ── 10. QSearch ──
    h1("10. Quiescence Search")
    n = count(con, "qs")
    if n >= 50:
        df = query(con, "qs", [
            F("p", "INTEGER"), F("nc", "INTEGER"), F("dp", "INTEGER"),
            F("se", "INTEGER"), F("fc", "BOOLEAN"), F("ck", "BOOLEAN"),
        ])
        if df is not None and len(df) > 0:
            p(f"**Avg captures searched per QNode:** {df['nc'].mean():.1f}")
            p(f"**Max captures in one QNode:** {int(df['nc'].max())}")
            p(f"**Futility cutoff rate:** {df['fc'].mean()*100:.1f}%")
            p(f"**Delta prune rate:** {df['dp'].sum() / (df['dp'].sum() + len(df)) * 100:.1f}% of QNodes delta-pruned at least one move")
            denom = df['se'].sum() + df['nc'].sum()
            p(f"**SEE prune rate in QSearch:** {df['se'].sum() / max(denom, 1) * 100:.1f}% of captures SEE-pruned")
            if df["nc"].mean() > 5:
                warn(f"QSearch explosion: avg {df['nc'].mean():.1f} captures per node — tighten margins.")
            elif df["nc"].mean() < 1.5:
                note("QSearch very quiet — delta/futility margins may be too aggressive, could miss tactics.")
            else:
                note("QSearch capture count is healthy (1.5-5 range).")
        else:
            warn("QSearch query failed.")
    else:
        warn(f"Only {n} QSearch events.")

    # ── 11. Correction ──
    h1("11. Correction History")
    n = count(con, "cr")
    if n >= 20:
        df = query(con, "cr", [
            F("cv", "INTEGER"), F("re", "INTEGER"), F("ce", "INTEGER"),
            F("df", "INTEGER"), F("pc", "INTEGER"), F("np", "INTEGER"),
            F("cc", "INTEGER"), F("pl", "INTEGER"),
        ])
        if df is not None and len(df) > 0:
            p(f"**Correction stats:** mean={df['cv'].mean()/512:.1f} cp, median={df['cv'].median()/512:.1f} cp, stddev={df['cv'].std()/512:.1f} cp")
            p(f"**Avg absolute correction:** {df['cv'].abs().mean()/512:.1f} cp")
            p(f"**Correction range:** [{df['cv'].min()/512:.1f}, {df['cv'].max()/512:.1f}] cp")
            for col, nm in [("pc", "Pawn"), ("np", "Non-Pawn"), ("cc", "Continuation")]:
                if col in df.columns:
                    p(f"- **{nm} component:** mean={df[col].mean()/512:.1f} cp, σ={df[col].std()/512:.1f}")
            avg_c = df["cv"].abs().mean() / 512
            if avg_c > 50:
                warn("Large corrections — eval has systematic biases.")
            elif avg_c > 20:
                note("Moderate corrections — eval is decent but improvable.")
            else:
                note("Small corrections — eval is well-calibrated.")
        else:
            warn("Correction query failed.")
    else:
        warn(f"Only {n} correction events — `cr` probe not wired. Add `probe!()` in `correction.rs`.")

    # ── 12. Continuation History ──
    h1("12. Continuation History")
    n = count(con, "ch")
    if n >= 5:
        df = query(con, "ch", [F("tb", "VARCHAR"), F("hr", "DOUBLE"), F("as", "DOUBLE"),
                                F("mx", "INTEGER"), F("uf", "BIGINT")])
        if df is not None and len(df) > 0:
            df["hr_pct"] = df["hr"] * 100
            p(_md_table(df, ["tb", "hr_pct", "as", "mx", "uf"]))
            for _, r in df.iterrows():
                if r["hr"] < 0.3:
                    warn(f"{r['tb']}: hit rate {r['hr']*100:.1f}% — very low. Table may be too small or update freq too low.")
        else:
            warn("Cont. history query failed.")
    else:
        warn(f"Only {n} cont. history events — `ch` probe not wired.")


    # ── 13. Tactical Extensions ──
    h1("13. Tactical Extensions")
    
    if count(con, "sx") > 0:
        h2("Singular Extensions")
        df_sx = query(con, "sx", [F("d", "INTEGER"), F("tt", "INTEGER"), F("sb", "INTEGER"), F("ss", "INTEGER"), F("ext", "INTEGER"), F("mc", "BOOLEAN")])
        p(f"- **Total triggers:** {len(df_sx):,}")
        p(f"- **Extensions applied:** {df_sx['ext'].sum():,} ({df_sx['ext'].mean()*100:.1f}%)")
        p(f"- **Multi-cut events:** {df_sx['mc'].sum():,} ({df_sx['mc'].mean()*100:.1f}%)")
        p(f"- **Negative extensions:** {(len(df_sx) - df_sx['ext'].sum() - df_sx['mc'].sum()):,} ({(len(df_sx) - df_sx['ext'].sum() - df_sx['mc'].sum())/len(df_sx)*100:.1f}%)")
    else:
        warn("No Singular Extension (`sx`) events found.")
        
    if count(con, "te") > 0:
        h2("Threat Extensions")
        df_te = query(con, "te", [F("d", "INTEGER"), F("lr", "INTEGER")])
        p(f"- **Threat extensions triggered:** {len(df_te):,}")
    else:
        warn("No Threat Extension (`te`) events found.")
        
    if count(con, "re") > 0:
        h2("Recapture Extensions")
        df_re = query(con, "re", [F("d", "INTEGER")])
        p(f"- **Recapture extensions triggered:** {len(df_re):,}")
    else:
        warn("No Recapture Extension (`re`) events found.")

    # ── Event Inventory ──
    h1("A. Event Inventory")
    types = ["cf", "b", "mg", "ev", "sn", "ss", "tt", "tc", "fp", "rp", "lm",
             "nm", "se", "qs", "aw", "ii", "mo", "ht", "rt", "tm", "tz", "dd", "md", "ch", "cr", "sx", "te", "re"]
    names = {
        "cf": "Config", "b": "Board", "mg": "Movegen", "ev": "Eval",
        "sn": "Search Node", "ss": "Search Summary", "tt": "TT Probe",
        "tc": "TT Cutoff", "fp": "FFP", "rp": "RFP", "lm": "LMR",
        "nm": "Null Move", "se": "SEE", "qs": "Quiescence", "aw": "Aspiration",
        "ii": "IID", "mo": "Move Ordering", "ht": "History Table", "rt": "Root",
        "tm": "Time Mgmt", "tz": "Syzygy", "dd": "Draw Detection",
        "md": "Mate Distance", "ch": "Cont. History", "cr": "Correction",
        "sx": "Singular Ext", "te": "Threat Ext", "re": "Recapture Ext",
    }
    total = 0
    for t in types:
        c = count(con, t)
        total += c
        status = "✅" if c > 0 else "❌ NOT WIRED"
        md.append(f"- `{t}` ({names.get(t, '?')}): {c:,} {status}")
    p(f"\n**Total:** {total:,} events across {len(types)} types.")

    # Footer
    md.append(f"\n---\n*Generated {__import__('datetime').datetime.now().strftime('%Y-%m-%d %H:%M:%S')} from {len(paths)} probe file(s)*")
    return "\n".join(md)


# ── main ────────────────────────────────────────────────────────────────────
def main():
    parser = argparse.ArgumentParser(description="Analyze Boa probe JSONL files")
    parser.add_argument("files", nargs="+", help="Probe JSONL file(s), globs supported")
    parser.add_argument("-o", "--output", default="probe-report.html", help="Output file")
    parser.add_argument("-t", "--text", action="store_true",
                        help="Generate markdown text report (for LLM ingestion)")
    args = parser.parse_args()

    # Default output name based on format
    if args.output == "probe-report.html" and args.text:
        args.output = "probe-report.md"

    print(f"Loading {len(args.files)} pattern(s)...")
    con, field_legend, paths = load_probe_data(args.files)
    print(f"  {len(paths)} file(s) loaded, {count(con, 'ss')} search summaries found.")

    if args.text:
        print("Generating markdown report...")
        report = text_report(con, paths)
        with open(args.output, "w") as f:
            f.write(report)
        print(f"\nReport written to {args.output} ({len(report):,} chars)")
        print(f"Paste into ChatGPT/Claude or open with any text editor.")
        # Also print to stdout so user can pipe it
        print(f"\nPreview (first 80 lines):")
        for i, line in enumerate(report.split("\n")[:80]):
            print(line)
    else:
        sections = [
            ("1. Search Overview", section_search_summary(con, paths)),
            ("2. Iteration Timeline", section_iteration_timeline(con)),
            ("3. Pruning System", section_pruning(con)),
            ("4. Move Ordering", section_move_ordering(con)),
            ("5. Evaluation Breakdown", section_eval(con)),
            ("6. Search Stability", section_stability(con)),
            ("7. Transposition Table", section_tt(con)),
            ("8. Node Characteristics", section_nodes(con)),
            ("9. Static Exchange Evaluation", section_see(con)),
            ("10. Quiescence Search", section_qsearch(con)),
            ("11. Correction History", section_correction(con)),
            ("12. Continuation History", section_continuation_history(con)),
            ("A. Event Inventory", section_inventory(con, paths)),
        ]

        body = ""
        for title, content in sections:
            if content:
                body += f"<h2>{title}</h2>\n{content}\n"

        fnames = ", ".join(Path(p).name for p in paths[:5])
        if len(paths) > 5:
            fnames += f" (+{len(paths) - 5} more)"

        html = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Boa Probe Report — {fnames}</title>
<script src="https://cdn.plot.ly/plotly-3.0.1.min.js"></script>
{CSS}
</head>
<body>
<h1>Boa Engine Probe Report</h1>
<p class="subtitle">Generated from {fnames}</p>
{body}
<div class="footer">Boa probe analyzer — generated {__import__('datetime').datetime.now().strftime('%Y-%m-%d %H:%M:%S')}</div>
</body>
</html>"""

        with open(args.output, "w") as f:
            f.write(html)
        print(f"\nReport written to {args.output} ({len(html):,} bytes)")
        print(f"Open with: xdg-open {args.output}")


if __name__ == "__main__":
    main()
