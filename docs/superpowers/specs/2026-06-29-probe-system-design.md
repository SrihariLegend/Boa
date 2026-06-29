# Probe System Design — Bone-Deep Engine Diagnostics

**Date:** 2026-06-29
**Status:** Approved — awaiting implementation plan

## Overview

A feature-gated (`--features probes`) diagnostic system that streams every
decision from every module to a JSONL file.  Zero runtime cost when disabled.
Designed for AI ingestion: short field codes, one event per line, statistical
by default with opt-in per-node verbosity.

## Architecture

```
┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐
│  Board   │  │ Movegen  │  │  Eval    │  │ Search   │
│ probe!() │  │ probe!() │  │ probe!() │  │ probe!() │
└────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘
     │              │              │              │
     ▼              ▼              ▼              ▼
┌────────────────────────────────────────────────────┐
│  ProbeBus: crossbeam::unbounded()                  │
│  Lock-free MPMC — push never blocks search thread  │
└──────────────────────┬─────────────────────────────┘
                       │ drain
                       ▼
              ┌──────────────────────┐
              │  Writer thread        │
              │  BufWriter → file     │
              │  logs/boa-probe-      │
              │  <timestamp>.jsonl     │
              │                       │
              │  Drops events if      │
              │  channel is full      │
              │  (increments dropped)  │
              └──────────────────────┘
```

Key properties:
- **Lock-free probe!()**: `crossbeam::unbounded().send()` — no blocking in the hot path.
- **`#[cfg(feature = "probes")]`**: the `probe!` macro compiles to nothing in release builds.
- **Drain on full**: if the channel backs up, events are dropped and a `dropped` counter
  increments.  The per-search summary reports the drop count.
- **Writer thread**: single consumer, `BufWriter` for amortized I/O. Flushes every 100ms
  or when the search completes.

## Feature Flag

```toml
# Cargo.toml
[features]
probes = []
```

Usage:
```sh
cargo build --release --features probes   # engine with probes enabled
cargo build --release                      # production — zero overhead
```

## ProbeBus API

Module location: `src/probe/mod.rs` (new top-level module, added to `lib.rs`).

Channel choice: `std::sync::mpsc::sync_channel(8192)` — bounded to limit memory,
non-blocking `try_send()`.  No new dependency needed (crossbeam not required).

```rust
// src/probe/mod.rs

pub struct ProbeBus {
    tx: std::sync::mpsc::SyncSender<ProbeEvent>,
    dropped: AtomicU64,
}

impl ProbeBus {
    /// Spawns the writer thread and returns the bus.
    /// `writer` is typically a BufWriter<File> opened in logs/.
    pub fn new(writer: impl Write + Send + 'static) -> Self;

    /// Non-blocking send.  If the channel is full, increments `dropped` and discards.
    pub fn send(&self, event: ProbeEvent);

    /// Number of events dropped due to channel full.
    pub fn dropped(&self) -> u64;

    /// Sends a sentinel event that tells the writer thread to flush and finish.
    /// Called at the end of each search.
    pub fn finish(&self);
}

// The probe! macro — call from any module:
//
//   probe!(ctx, Ffp { depth, sigma, margin, required_gain, pruned });
//
// Expands to:
//
//   #[cfg(feature = "probes")]
//   if let Some(ref bus) = ctx.probe_bus {
//       bus.send(ProbeEvent::Ffp(FfpEvent { depth, sigma, margin, required_gain, pruned }));
//   }
//
// Defined as:
macro_rules! probe {
    ($ctx:expr, $variant:ident { $($field:ident : $value:expr),* $(,)? }) => {
        #[cfg(feature = "probes")]
        if let Some(ref bus) = $ctx.probe_bus {
            bus.send(ProbeEvent::$variant(
                $crate::probe::events::$variant##Event { $($field: $value),* }
            ));
        }
    };
}
```

The writer thread:
- Reads events from the channel receiver
- Serializes each to one JSON line via `serde_json::to_writer`
- Flushes every 100ms or on receiving a `Finish` sentinel event
- On `Finish`: flushes, closes the file, exits the thread

## File Format

Each file starts with a single `meta` event declaring the field legend,
then one JSON object per line.

```jsonl
{"typ":"meta","fields":{"fp":{"d":"depth","sg":"sigma","mg":"margin","rg":"required_gain","pr":"pruned"}}}
{"typ":"cf","ms":16,"tt":16,"ms":100,"ps":100,"ks":100,"mo":100,"pa":100}
{"typ":"b","f":"rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b","p":0,"mo":14,"of":0}
{"typ":"ev","ph":0,"ma_mg":0,"ma_eg":0,"ma_cp":0,"ps_mg":38,"ps_eg":-12,"ps_cp":38,...}
{"typ":"fp","d":3,"mi":22,"sg":18,"mg":168,"rg":55,"pr":false}
```

## Short Field Codes Convention

- Single-character for the most common fields: `d`=depth, `p`=ply, `e`=eval, `a`=alpha, `b`=beta
- Two-character `xx_yy` for decomposed values: `ma_mg`=material midgame, `ma_eg`=material endgame
- Boolean fields are `true`/`false` (not 1/0) for readability
- All fields are `snake_case` short codes; the abbreviation is in the meta header

## Per-Module Event Schemas

### 1. Config — `typ:"cf"`
Fires once at search start.

| Code | Field | Type |
|------|-------|------|
| ms | tt_size_mb | u32 |
| ma | material_scale | i32 |
| ps | pst_scale | i32 |
| mo | mobility_scale | i32 |
| ks | king_safety_scale | i32 |
| pa | pawn_structure_scale | i32 |
| co | contempt | i32 |
| sy | syzygy_enabled | bool |
| md | max_depth | u32 |
| mt | move_time | u64 |
| wt | wtime | i64 |
| bt | btime | i64 |
| wi | winc | i64 |
| bi | binc | i64 |
| mg | moves_to_go | i32 |

### 2. Board — `typ:"b"`
Fires on position set (UCI `position` command), and optionally per-node at low depth.

| Code | Field | Type |
|------|-------|------|
| f | fen | str (truncated 64) |
| p | phase [0-24] | i32 |
| nm | non_pawn_material | i32 |
| mo | mobile_pieces | i32 |
| of | open_files | i32 |
| ck | in_check | bool |
| mr | material_rule_score | i32 |
| hm | halfmove_clock | i32 |
| fl | fullmove_number | i32 |

### 3. Movegen — `typ:"mg"`
Fires per position.

| Code | Field | Type |
|------|-------|------|
| nc | total_move_count | u32 |
| qc | quiet_count | u32 |
| cc | capture_count | u32 |
| ec | evasion_count | u32 |
| pc | promotion_count | u32 |
| ck | in_check | bool |

### 4. Eval — `typ:"ev"`
Full `EvalBreakdown`. Fires per node (sampled by depth — always at d≤2, 1-in-8 at d≥3).

| Code | Field | Type |
|------|-------|------|
| ph | phase [0-256] | i32 |
| ma_mg | material_mg | i32 |
| ma_eg | material_eg | i32 |
| ma_cp | material_cp | i32 |
| ps_mg | pst_mg | i32 |
| ps_eg | pst_eg | i32 |
| ps_cp | pst_cp | i32 |
| mo_mg | mobility_mg | i32 |
| mo_eg | mobility_eg | i32 |
| mo_cp | mobility_cp | i32 |
| mw | mobility_white | u32 |
| mb | mobility_black | u32 |
| pa_mg | pawn_structure_mg | i32 |
| pa_eg | pawn_structure_eg | i32 |
| pa_cp | pawn_structure_cp | i32 |
| ks_mg | king_safety_mg | i32 |
| ks_eg | king_safety_eg | i32 |
| ks_cp | king_safety_cp | i32 |
| fr | freedom | i32 |
| td_mg | trade_down_mg | i32 |
| td_eg | trade_down_eg | i32 |
| td_cp | trade_down_cp | i32 |
| ws_mg | weak_squares_mg | i32 |
| ws_eg | weak_squares_eg | i32 |
| ws_cp | weak_squares_cp | i32 |
| co_mg | coordination_mg | i32 |
| co_eg | coordination_eg | i32 |
| co_cp | coordination_cp | i32 |
| ap_mg | advanced_pawns_mg | i32 |
| ap_eg | advanced_pawns_eg | i32 |
| ap_cp | advanced_pawns_cp | i32 |
| ws | white_score | i32 |
| ss | side_to_move_score | i32 |

### 5. Search Node — `typ:"sn"`
Fires per node (sampled: always at ply≤4, 1-in-16 at ply≥5).

| Code | Field | Type |
|------|-------|------|
| d | depth | i32 |
| p | ply | u32 |
| se | static_eval | i32 |
| a | alpha | i32 |
| b | beta | i32 |
| pv | is_pv | bool |
| cu | is_cut_node | bool |
| ck | in_check | bool |
| im | improving | bool |
| ps | prev_static_eval | Option<i32> |
| sc | score (result) | i32 |
| nm | moves_searched | u32 |
| bf | beta_cutoffs_this_node | u32 |
| fc | first_move_cutoff | bool |
| tm | node_time_us | u64 |
| tb | tb_hit | bool |
| tt | tt_hit | bool |

### 6. Search Summary — `typ:"ss"`
Fires once at search end. All counters from `SearchStats` plus derived rates.

| Code | Field | Type |
|------|-------|------|
| td | depth_completed | i32 |
| ns | total_nodes | u64 |
| qs | qsearch_nodes | u64 |
| tm | time_ms | u64 |
| np | nodes_per_sec | u64 |
| bm | best_move (UCI) | str |
| bs | best_score | i32 |
| sd | sel_depth | i32 |
| tt_p | tt_probes | u64 |
| tt_h | tt_hits | u64 |
| tt_c | tt_cutoffs | u64 |
| bc | beta_cutoffs | u64 |
| fc | first_move_cutoffs | u64 |
| nm_t | null_move_tries | u64 |
| nm_c | null_move_cutoffs | u64 |
| rp | rfp_cutoffs | u64 |
| fp_a | ffp_attempts | u64 |
| fp_p | ffp_prunes | u64 |
| lm_a | lmr_attempts | u64 |
| lm_r | lmr_actual_reductions | u64 |
| lm_rs | lmr_re_searches | u64 |
| se_w | see_win_caps | u64 |
| se_e | see_equal_caps | u64 |
| se_l | see_loss_caps | u64 |
| se_s | see_loss_searched | u64 |
| ii_t | iid_triggers | u64 |
| ii_s | iid_successes | u64 |
| tb_h | tb_hits | u64 |
| dr | dropped_probe_events | u64 |

### 7. TT Probe — `typ:"tt"`
Fires on each TT probe (sampled 1-in-32 at depth≥5, always at depth≤4).

| Code | Field | Type |
|------|-------|------|
| op | operation (probe/store) | str |
| h | hit | bool |
| et | entry_type (exact/alpha/beta/empty) | str |
| ed | entry_depth | i8 |
| es | entry_score | i32 |
| ag | entry_age | u8 |
| si | slot_index | u8 |
| re | replaced (bool — was an older entry overwritten) | bool |
| rd | replaced_depth | i8 |

### 8. TT Cutoff — `typ:"tc"`
Fires on each TT cutoff decision.

| Code | Field | Type |
|------|-------|------|
| d | depth | i32 |
| et | entry_type | str |
| ed | entry_depth | i8 |
| df | depth_sufficient (ed ≥ d) | bool |
| sc | cutoff_score | i32 |
| a | alpha | i32 |
| b | beta | i32 |

### 9. FFP — `typ:"fp"`
Fires per quiet move considered for FFP (sampled 1-in-8 at d≥3, always at d≤2).

| Code | Field | Type |
|------|-------|------|
| d | depth | i32 |
| mi | move_index | u32 |
| hs | history_score | i32 |
| sg | sigma | i32 |
| mg | computed_margin | i32 |
| rg | required_gain (α - static_eval) | i32 |
| pr | pruned | bool |
| cu | is_cut_node | bool |

### 10. RFP — `typ:"rp"`
Fires on each RFP decision (always — fires ~300 times per search).

| Code | Field | Type |
|------|-------|------|
| d | depth | i32 |
| se | static_eval | i32 |
| b | beta | i32 |
| sg | sigma | i32 |
| mg | computed_margin | i32 |
| pr | pruned | bool |

### 11. LMR — `typ:"lm"`
Fires per quiet move that reaches LMR consideration (sampled 1-in-4).

| Code | Field | Type |
|------|-------|------|
| d | depth | i32 |
| p | ply | u32 |
| mi | move_index | u32 |
| ms | moves_searched | u32 |
| hs | history_score | i32 |
| br | base_reduction | i32 |
| ar | actual_reduction | i32 |
| nd | new_depth | i32 |
| cs | criticality_score | f64 |
| cp | protected_by_criticality | bool |
| ip | improving | bool |
| ki | is_killer | bool |
| co | is_counter | bool |
| tm | tt_move_agreement | bool |
| gc | gives_check | bool |
| pi | moving_piece | u8 |
| cu | is_cut_node | bool |

### 12. Null Move — `typ:"nm"`
Fires on each null move attempt.

| Code | Field | Type |
|------|-------|------|
| d | depth | i32 |
| se | static_eval | i32 |
| b | beta | i32 |
| r | reduction | i32 |
| sc | null_move_score | i32 |
| pr | pruned (cutoff achieved) | bool |

### 13. Variance — `typ:"va"`
Fires on each σ(pos) computation (always — cheap, ~once per node).

| Code | Field | Type |
|------|-------|------|
| sg | sigma | i32 |
| fm | f_mobility | f64 |
| fo | f_open | f64 |
| fp | f_phase | f64 |
| mo | mobile_piece_count | i32 |
| of | open_file_count | i32 |
| np | non_pawn_material | i32 |
| ph | phase | i32 |

### 14. SEE — `typ:"se"`
Fires on SEE evaluation of captures (sampled 1-in-8).

| Code | Field | Type |
|------|-------|------|
| vl | see_value | i32 |
| cv | captured_value | i32 |
| th | threshold (for pruning decision) | i32 |
| pr | pruned_by_see | bool |
| sr | searched_despite_bad_see | bool |

### 15. Quiescence — `typ:"qs"`
Fires per q-search node (sampled 1-in-32).

| Code | Field | Type |
|------|-------|------|
| p | ply | u32 |
| sp | stand_pat_score | i32 |
| a | alpha | i32 |
| b | beta | i32 |
| sc | final_score | i32 |
| nc | captures_searched | u32 |
| dp | delta_pruned_count | u32 |
| se | see_pruned_count | u32 |
| ck | in_check | bool |
| fc | futility_cutoff | bool |

### 16. Aspiration — `typ:"aw"`
Fires on aspiration window events at root.

| Code | Field | Type |
|------|-------|------|
| d | depth | i32 |
| dl | initial_delta | i32 |
| lo | window_low | i32 |
| hi | window_high | i32 |
| fh | fail_high | bool |
| fl | fail_low | bool |
| ex | expansion_count | u32 |
| rs | research_score | i32 |

### 17. IID — `typ:"ii"`
Fires on IID trigger.

| Code | Field | Type |
|------|-------|------|
| d | depth | i32 |
| rd | reduced_depth | i32 |
| tf | tt_move_found_after_iid | bool |
| sc | iid_search_score | i32 |

### 18. Move Ordering — `typ:"mo"`
Fires per scored move (sampled 1-in-16).

| Code | Field | Type |
|------|-------|------|
| p | ply | u32 |
| mi | move_index | u32 |
| ph | phase_picked (tt/hash/good_cap/killer/counter/quiet/bad_cap) | str |
| bf | butterfly_score | i32 |
| kh | killer_score | i32 |
| co | counter_score | i32 |
| ch | capture_history_score | i32 |
| mv | mvv_lva_base | i32 |
| tt | tt_move_bonus | bool |
| pr | promotion_bonus | bool |

### 19. History Table — `typ:"ht"`
Fires on history table management events (rare).

| Code | Field | Type |
|------|-------|------|
| ev | event_type (overflow/scale_down/cap_scale_down) | str |
| ci | color_index | u8 |
| pi | piece_index | u8 |
| mx | max_value_before | i32 |
| mn | min_value_before | i32 |
| th | threshold | i32 |

### 20. Root — `typ:"rt"`
Fires per root iteration.

| Code | Field | Type |
|------|-------|------|
| d | depth | i32 |
| bm | best_move (UCI) | str |
| bs | best_score | i32 |
| pv | pv_line (first 5 moves UCI) | str |
| bc | best_move_changed | bool |
| pc | previous_best_move | str |
| tm | iteration_time_ms | u64 |
| ns | nodes_this_iteration | u64 |
| af | aspiration_fails | u32 |

### 21. Time Management — `typ:"tm"`
Fires at move allocation time.

| Code | Field | Type |
|------|-------|------|
| al | allocated (soft budget) | u64 |
| ha | hard_limit | u64 |
| op | optimum_time | u64 |
| el | elapsed | u64 |
| mt | moves_to_go | i32 |
| mp | move_overhead | i64 |
| rm | remaining_clock | i64 |
| ic | increment | i64 |

### 22. Syzygy — `typ:"tz"`
Fires on tablebase probe.

| Code | Field | Type |
|------|-------|------|
| rs | result (win/draw/loss/cursed/blessed/not_found) | str |
| dm | distance_to_mate | i32 |
| pc | piece_count | u8 |
| dz | dtz_value | i32 |
| wp | wdl_probe_success | bool |

### 23. Draw Detection — `typ:"dd"`
Fires when a draw is detected during search.

| Code | Field | Type |
|------|-------|------|
| ty | draw_type (repetition/fifty_move/insufficient_material) | str |
| p | ply | u32 |
| co | contempt_applied | i32 |
| sc | score_returned | i32 |

### 24. Mate Distance — `typ:"md"`
Fires when mate distance pruning clamps alpha/beta.

| Code | Field | Type |
|------|-------|------|
| p | ply | u32 |
| oa | original_alpha | i32 |
| na | clamped_alpha | i32 |
| ob | original_beta | i32 |
| nb | clamped_beta | i32 |
| pr | pruned (alpha ≥ beta after clamping) | bool |

## Sampling Strategy

Some events fire millions of times per search (Search Node, Eval, Move Ordering).
Default sampling:

| Event | Sampling |
|-------|----------|
| cf, ss, tm, uc | always (once per search or move) |
| b, mg | always (once per position, ~60 per game) |
| rp, fp (d≤2), va, dd, md, tc | always (rare but cheap) |
| fp (d≥3), lm, se, nm | 1-in-8 |
| sn (ply≤4), tt (d≤4) | always |
| sn (ply≥5), tt (d≥5), ev | 1-in-16 |
| qs, mo | 1-in-32 |
| aw, ii, ht, tz, rt | always (rare) |

Override via env var: `BOA_PROBE_SAMPLE=all` disables all sampling (every event fires).

## Extensibility

### Adding a new field to an existing module

Add the field as `Option<T>` with `#[serde(skip_serializing_if = "Option::is_none")]`:

```rust
struct FfpEvent {
    depth: i32,
    sigma: i32,
    // NEW FIELD — old logs parse fine, new logs get it
    #[serde(skip_serializing_if = "Option::is_none")]
    new_field: Option<i32>,
}
```

Then add the short code to the `meta` header.  That's it.

### Adding a new module

1. Define the event struct in `src/probe/events.rs`
2. Add variant to `ProbeEvent` enum
3. Add the `typ` short code and field legend in `ProbeEvent::to_json()`
4. Call `probe!(bus, NewModule { field, .. })` in the module's hot path
5. Add `pub mod` and `pub use` in `src/probe/mod.rs`

~15 lines of boilerplate.  The `probe!` macro handles the `#[cfg]` gating.

## Integration Points

Where `probe!()` calls go in each module:

| Module | File | Location |
|--------|------|----------|
| Config | `src/search/mod.rs` | After `SearchContext::new()`, pass probe_bus in context |
| Board | `src/uci/position.rs` | After `handle_position` sets the board |
| Movegen | `src/search/alpha_beta.rs` | After `gen_moves()` |
| Eval | `src/eval/types.rs` | Inside `evaluate_breakdown()`, after building EvalBreakdown |
| Search Node | `src/search/alpha_beta.rs` | After node result is known |
| Search Summary | `src/search/root.rs` | End of `search()` or `search_single()` |
| TT | `src/tt/table.rs` | Inside `probe()` and `store()` |
| TT Cutoff | `src/search/tt_cutoff.rs` | Inside `try_tt_cutoff()` |
| FFP | `src/search/pruning/ffp.rs` | Inside `should_ffp_prune()` |
| RFP | `src/search/alpha_beta.rs` | At RFP decision point |
| LMR | `src/search/pruning/lmr.rs` | Inside LMR reduction logic |
| Null Move | `src/search/null_move.rs` | Inside `try_null_move()` |
| Variance | `src/search/pruning/variance.rs` | Inside `sigma()` |
| SEE | `src/search/see.rs` | Inside `static_exchange_eval()` |
| Quiescence | `src/search/quiescence.rs` | Inside `quiescence()` |
| Aspiration | `src/search/root.rs` | Inside `aspiration_search()` |
| IID | `src/search/alpha_beta.rs` | At IID decision |
| Move Ordering | `src/search/move_ordering.rs` | Inside `score_single_move()` |
| History | `src/search/move_ordering.rs` | Inside scale-down functions |
| Root | `src/search/root.rs` | End of each iteration |
| Time Mgmt | `src/search/context.rs` | Inside `time_for_move()` |
| Syzygy | `src/syzygy/probe.rs` | Inside probe functions |
| Draw | `src/search/alpha_beta.rs` | At draw detection points |
| Mate Distance | `src/search/alpha_beta.rs` | At mate distance pruning |

## SearchContext Changes

`SearchContext` gains an optional `ProbeBus`:

```rust
pub struct SearchContext<'a> {
    // ... existing fields ...
    #[cfg(feature = "probes")]
    pub probe_bus: Option<&'a ProbeBus>,
}
```

The `probe!` macro checks `ctx.probe_bus` is `Some` before sending.

## File Output

- Directory: `logs/` (created at engine startup if probes enabled)
- Filename: `boa-probe-YYYY-MM-DD-HHMMSS.jsonl`
- New file per UCI `go` command for cleanliness
- A `boa-probe-meta.jsonl` in the directory with field code reference

## New Dependencies

```toml
# Cargo.toml (gated behind probes feature)
[features]
probes = ["serde", "serde_json"]

[dependencies]
serde = { version = "1", features = ["derive"], optional = true }
serde_json = { version = "1", optional = true }
```

`serde`/`serde_json` are only compiled when `--features probes` is active.

## File Structure

```
src/probe/
  mod.rs         — ProbeBus, macro_rules! probe!, writer thread
  events.rs      — ProbeEvent enum, all per-module event structs, Serialize impls
  legend.rs      — generates the per-file meta header from event struct field annotations
```

## What Belongs in CLAUDE.md / AGENTS.md

Add to AGENTS.md:
```
## Probe System

When adding a new module to the engine, you MUST add probe events for it:
1. Define event struct in src/probe/events.rs
2. Add variant to ProbeEvent enum
3. Add probe!() calls in the module's key decision points
4. Add the module to the coverage table in docs/superpowers/specs/2026-06-29-probe-system-design.md
```

## Non-Goals

- No real-time dashboard (can be built later as a separate tool that tails the JSONL)
- No binary format (JSONL is AI-friendly and tool-able)
- No network streaming (file-based is simpler and sufficient for diagnosis)
- No replacement of criticality logger (the criticality CSV is a separate concern for ML training)
