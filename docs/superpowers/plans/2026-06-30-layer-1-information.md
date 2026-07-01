# Layer 1 — Information Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the signal infrastructure that every selectivity and pruning decision depends on — history bonus formula upgrade, continuation history (1/2/4/6-ply), pawn history, and correction history.

**Architecture:** Six sequential tasks, each SPRT-validated before the next begins. The history bonus formula is upgraded first (the magnitude of updates changes), then continuation history tables are added incrementally (1-ply → 2-ply → 4/6-ply), then pawn history, then correction history. Each continuation history offset uses an index-based propagation pattern through `PlyInfo` to avoid Rust lifetime issues with self-referential structs. Correction history computes an online bias correction to static eval and applies it before all pruning decisions.

**Tech Stack:** Rust (2021 edition), existing bitboard/tt modules. No new dependencies.

## Global Constraints

- Correctness before optimization — no heuristic can be trusted on a broken foundation (Engineering Oath #1)
- Information quality before selectivity — pruning decisions are only as good as the signals they read (Engineering Oath #2)
- Architecture before heuristics — build the structural layer a heuristic depends on before implementing the heuristic (Engineering Oath #3)
- Measurement before intuition — every decision has a measurement behind it (Engineering Oath #4)
- One change per SPRT — never bundle multiple changes in a single SPRT test
- `cargo test` must pass after every task
- SPRT at fast time control (1+0.01s) for each task that changes search behavior
- Every subsystem must expose probe diagnostics before it is tuned (Engineering Oath #7)
- Use Rust 2021 idioms and `rustfmt` formatting
- Add tests beside the code under `#[cfg(test)] mod tests`
- Commit messages must end with `Co-Authored-By: Claude <noreply@anthropic.com>`
- Do not commit `target/`, `analysis/`, `__pycache__/`, `*.pyc`, `*.log`
- Retain existing killer moves and counter-move heuristic — do not remove or reduce their weight during Layer 1

---

### Task 1: History Bonus Formula Upgrade

**Files:**
- Modify: `src/search/move_ordering.rs:122-131` — replace `history_delta()` and `history_malus()` with Obsidian-style formulas
- Modify: `src/search/move_ordering.rs:178-207` — update `handle_beta_cutoff()` signature and body to pass `is_strong_cutoff`
- Modify: `src/search/move_ordering.rs:151-176` — update `update_cap_history()` to use new bonus formula
- Modify: `src/search/alpha_beta.rs:554-555` — update malus call site (formula changes, call site stays same since `history_malus` keeps its signature)
- Modify: `src/search/alpha_beta.rs:570-577` — update `handle_beta_cutoff()` call to pass `best_score` and `beta`
- Test: `src/search/move_ordering_tests.rs` — add tests for new formulas

**Interfaces:**
- Consumes: `history_delta(depth) -> i32` (old), `history_malus(depth) -> i32` (old)
- Produces: `history_delta(depth: i32, is_strong_cutoff: bool) -> i32` (new — adds `is_strong_cutoff` param)
- Produces: `history_malus(depth: i32) -> i32` (new formula, same signature)
- Produces: `handle_beta_cutoff(ctx, board, m, ply, depth, is_capture, best_score, beta)` (new — adds `best_score` and `beta`)
- Produces: `update_cap_history(ctx, color, m, board, depth)` (same signature, new internal formula)

- [ ] **Step 1: Write failing tests for the new bonus formula**

Add to `src/search/move_ordering_tests.rs`:

```rust
#[test]
pub(in crate::search) fn history_delta_obsidian_formula() {
    // Obsidian formula: (175 * d + 15).min(1409)
    assert_eq!(history_delta(1, false), 175 * 1 + 15);  // 190
    assert_eq!(history_delta(1, true), 175 * 2 + 15);   // 365 (strong cutoff adds 1 to depth)
    assert_eq!(history_delta(2, false), 175 * 2 + 15);  // 365
    assert_eq!(history_delta(2, true), 175 * 3 + 15);   // 540
    assert_eq!(history_delta(5, false), 175 * 5 + 15);  // 890
    assert_eq!(history_delta(5, true), 175 * 6 + 15);  // 1065
    // Cap at 1409
    assert_eq!(history_delta(8, true), 1409);  // 175*9+15 = 1590, capped
    assert_eq!(history_delta(10, true), 1409); // well above cap
}

#[test]
pub(in crate::search) fn history_malus_obsidian_formula() {
    // Obsidian malus: -(196 * depth - 25).min(1047).max(-1047)
    assert_eq!(history_malus(1), -(196 * 1 - 25)); // -171
    assert_eq!(history_malus(2), -(196 * 2 - 25)); // -367
    assert_eq!(history_malus(5), -(196 * 5 - 25)); // -955
    assert_eq!(history_malus(6), -1047); // -(196*6-25) = -1151, clamped to -1047
    assert_eq!(history_malus(10), -1047); // well below clamp
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test history_delta_obsidian -- --nocapture 2>&1 | tail -20`
Expected: FAIL — old `depth * depth` formula returns different values (1→1, 2→4, 5→25, etc.)

Run: `cargo test history_malus_obsidian -- --nocapture 2>&1 | tail -20`
Expected: FAIL — old `-depth * depth` formula returns different values

- [ ] **Step 3: Replace `history_delta()` and `history_malus()`**

In `src/search/move_ordering.rs`, replace lines 122-131:

```rust
/// History bonus for the best quiet move on a beta cutoff.
/// Obsidian-style linear+cap formula: (175 * d + 15).min(1409).
/// `is_strong_cutoff` adds 1 to depth when best_score > beta + 75,
/// indicating a genuinely strong move worth a larger bonus.
pub(in crate::search) fn history_delta(depth: i32, is_strong_cutoff: bool) -> i32 {
    let d = depth + if is_strong_cutoff { 1 } else { 0 };
    (175 * d + 15).min(1409)
}

/// Malus (negative bonus) applied to quiet moves that were searched
/// but failed to cause a beta cutoff. Obsidian-style formula with
/// slightly larger magnitude than the bonus for asymmetry.
pub(in crate::search) fn history_malus(depth: i32) -> i32 {
    -(196 * depth - 25).min(1047).max(-1047)
}
```

- [ ] **Step 4: Update `update_cap_history()` to use the new bonus formula**

In `src/search/move_ordering.rs`, in `update_cap_history()` (line 171), replace:

```rust
// BEFORE (line 171):
let bonus = depth * depth;

// AFTER:
// Capture history uses the same bonus formula — captures that cause
// beta cutoffs are inherently strong, so is_strong_cutoff is always true.
let bonus = history_delta(depth, true);
```

- [ ] **Step 5: Update `handle_beta_cutoff()` signature and body**

In `src/search/move_ordering.rs`, replace lines 178-207 with:

```rust
/// Handle beta cutoff: update killers, history, counter moves.
/// `best_score` is the score that beat beta, used to compute is_strong_cutoff.
pub(in crate::search) fn handle_beta_cutoff(
    ctx: &mut SearchContext,
    board: &Board,
    m: Move,
    ply: usize,
    depth: i32,
    is_capture: bool,
    best_score: Score,
    beta: Score,
) {
    // Counterfactual probes are shadow-only: they may observe a full-depth
    // score, but must not train move-ordering heuristics used by the real search.
    if ctx.in_criticality_probe {
        return;
    }
    if is_capture {
        update_cap_history(ctx, board.side, m, board, depth);
        return;
    }
    update_killers(ctx, ply, m);
    let is_strong = best_score > beta + 75;
    let bonus = history_delta(depth, is_strong);
    let moving_piece = board.sq_piece[move_from(m) as usize];
    add_history_score(ctx, board.side, moving_piece, m, bonus);
    if ply == 0 || ply >= 128 {
        return;
    }
    let prev_move = ctx.stack[ply - 1].current_move;
    if prev_move != MOVE_NONE {
        ctx.counter[move_from(prev_move) as usize][move_to(prev_move) as usize] = m;
    }
}
```

- [ ] **Step 6: Update call sites in `alpha_beta.rs`**

In `src/search/alpha_beta.rs` at line 576, update the `handle_beta_cutoff` call:

```rust
// BEFORE (line 576):
handle_beta_cutoff(ctx, board, m, ply, depth, is_capture);

// AFTER:
handle_beta_cutoff(ctx, board, m, ply, depth, is_capture, score, beta);
```

The `score` variable is already in scope (it's the move loop iteration variable from `let score = ...`). The `beta` parameter is also in scope from the function arguments.

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test history_delta_obsidian history_malus_obsidian -- --nocapture 2>&1 | tail -20`
Expected: Both tests PASS

Run: `cargo test quiet_history_distribution -- --nocapture 2>&1 | tail -20`
Expected: Passes — history values still grow but with the new bonus magnitude

Run: `cargo test quiet_history_updates -- --nocapture 2>&1 | tail -20`
Expected: Passes — the test assertion `> 0` still holds (bonuses are different but still positive)

- [ ] **Step 8: Build and run full test suite**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compilation succeeds.

Run: `cargo test --quiet 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/search/move_ordering.rs src/search/alpha_beta.rs src/search/move_ordering_tests.rs
git commit -m "feat: upgrade history bonus formula to Obsidian-style linear+cap

Replace depth² bonus/malus with the Obsidian-style formulas:
- Bonus: (175 * d + 15).min(1409) with is_strong_cutoff depth bonus
- Malus: -(196 * depth - 25).min(1047).max(-1047)

The is_strong_cutoff flag (best_score > beta + 75) adds 1 to the effective
depth, giving a larger bonus to genuinely strong moves. Capture history
always uses is_strong_cutoff = true since captures that cause beta cutoffs
are inherently strong.

This matches the universal top-engine pattern (Stockfish, Obsidian, Ethereal,
Berserk all use linear+cap formulas). SPRT at fast time control expected
+3 to +8 Elo.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Continuation History (1-ply)

**Files:**
- Modify: `src/search/context.rs` — add `cont1` table to `SearchContext`, add `cont_entry` to `PlyInfo`
- Modify: `src/search/move_ordering.rs` — read cont1 score in `score_single_move()`, update cont1 in `handle_beta_cutoff()`
- Modify: `src/search/alpha_beta.rs` — set `cont_entry` in PlyInfo after move is made, before recursion
- Modify: `src/probe/events.rs` — add `ChEvent` for continuation history diagnostics
- Modify: `src/probe/mod.rs` — add `Ch` variant to `ProbeEvent` enum and field legend in `meta_json()`
- Test: `src/search/move_ordering_tests.rs` — add cont history tests

**Interfaces:**
- Consumes: `handle_beta_cutoff(ctx, board, m, ply, depth, is_capture, best_score, beta)` (from Task 1)
- Consumes: `add_history_score(ctx, color, piece, m, delta)` (existing)
- Consumes: `PlyInfo { current_move, static_eval }` (existing fields)
- Produces: `PlyInfo { current_move, static_eval, cont_entry: Option<(usize, usize)> }` — `(piece_type as usize, to_sq as usize)` of the move made at this ply
- Produces: `SearchContext.cont1: Box<[[[[i32; 64]; 6]; 64]; 6]>` — heap-allocated 576 KB table, initialized to -552
- Produces: Modified `score_single_move()` that adds continuation history score for quiet moves
- Produces: Modified `handle_beta_cutoff()` that updates cont1 via gravity formula
- Produces: `ChEvent` probe event with `hit_rate`, `avg_score`, `saturation`, `update_freq` fields

- [ ] **Step 1: Add continuation history state to `PlyInfo`**

In `src/search/context.rs`, modify the `PlyInfo` struct (line 50-54):

```rust
#[derive(Clone, Copy, Default)]
pub struct PlyInfo {
    pub current_move: Move,
    pub static_eval: Option<Score>,
    /// (piece_type as usize, to_sq as usize) of the move made at this ply.
    /// Used by continuation history — the child reads this from `stack[ply-1]`
    /// to get the previous move's piece and destination.
    pub cont_entry: Option<(usize, usize)>,
}
```

- [ ] **Step 2: Add `cont1` table to `SearchContext`**

In `src/search/context.rs`, add the field after `cap_history` (line 41):

```rust
// Continuation history 1-ply: [prev_piece][prev_to][piece][to] -> i32
// 6 x 64 x 6 x 64 = 147,456 entries, 576 KB. On the heap to avoid stack overflow.
pub cont1: Box<[[[[i32; 64]; 6]; 64]; 6]>,
```

In `SearchContext::new()` (around line 101, after `cap_history` init), add:

```rust
cont1: Box::new([[[[-552i32; 64]; 6]; 64]; 6]),
```

- [ ] **Step 3: Add `cont1` to the test_context helper**

In `src/search/test_utils.rs`, check and update `test_context()`:

```rust
// In test_context(), add after cap_history initialization:
cont1: Box::new([[[[-552i32; 64]; 6]; 64]; 6]),
```

Run to verify: `grep -n "fn test_context\|cap_history" src/search/test_utils.rs`

- [ ] **Step 4: Set `cont_entry` in PlyInfo before recursing**

In `src/search/alpha_beta.rs`, after a quiet move is made and before the recursive search, set `cont_entry` on the current ply's stack entry. The move is made at approximately line 321 (`let undo = board.make_move(m, ctx.z);`). After the move is made, we know `moving_piece` is valid.

Find the recursion point and add `cont_entry` setup. The key locations are:

**A. Full-depth search path** — before calling `alpha_beta()` for the child. In the full-depth search call (around line 460-470 in the existing code), add before the recursive call:

```rust
// Set continuation history entry for the child to read.
// Only set for quiet moves — captures use capture history, not cont history.
let prev_cont_entry = ctx.stack[ply].cont_entry;
if moving_piece != PIECE_NONE && !is_capture && !is_promo && ply + 1 < MAX_PLY {
    ctx.stack[ply].cont_entry = Some((
        piece_type(moving_piece) as usize,
        to as usize,
    ));
}
```

**B. Reduced search (LMR) path** — same setup before the reduced-depth recursive call.

Actually, the simplest approach: set `cont_entry` right after the board state is valid (after `make_move`), once per move, before ANY recursion. Find the line where `let undo = board.make_move(m, ctx.z);` is called (there may be one or two calls depending on whether LMR does a separate make/unmake).

Let's find the exact locations:

Run: `grep -n "make_move\|unmake_move\|alpha_beta(" src/search/alpha_beta.rs | head -30`

For now, the pattern is:

```rust
// After board.make_move(m, ctx.z) and before recursive alpha_beta() call:
if !is_capture && !is_promo && moving_piece != PIECE_NONE && ply + 1 < MAX_PLY {
    ctx.stack[ply].cont_entry = Some((
        piece_type(moving_piece) as usize,
        to as usize,
    ));
}
```

Important: save the old value and restore after recursion (in case the alpha_beta function reuses this ply's stack entry — it shouldn't since each ply has its own, but be safe):

```rust
let saved_cont_entry = ctx.stack[ply].cont_entry;
// ... set cont_entry ...
ctx.stack[ply].cont_entry = Some((...));
// ... recursive call ...
ctx.stack[ply].cont_entry = saved_cont_entry;
```

The exact placement will be determined by reading the current alpha_beta.rs structure. The plan specifies the pattern — the implementer reads the exact lines.

- [ ] **Step 5: Read continuation history score in `score_single_move()`**

In `src/search/move_ordering.rs`, modify `score_single_move()` to add continuation history for quiet moves. The function signature needs `ctx` and `ply` (it already takes `ctx` and `ply`). Add after the butterfly history (line 75) and before the final `s`:

```rust
// In score_single_move(), after the butterfly history line:
// Continuation history (1-ply): if the previous move exists, look up
// cont1[prev_piece][prev_to][current_piece][current_to].
// Only apply to quiet moves (not captures, not promotions, not TT move).
if ply > 0 && ply < 128 && m != tt_move && !is_capture && !is_promo {
    if let Some((pp, pto)) = ctx.stack[ply - 1].cont_entry {
        let mover_pt = piece_type(mover) as usize;
        let to_idx = move_to(m) as usize;
        s += ctx.cont1[pp][pto][mover_pt][to_idx];
    }
}
```

Note: `mover` is already computed earlier in the function (line 73). `is_promo` needs to be computed: add `let is_promo = move_flags(m) == MF_PROMOTION;` before the new block. `is_capture` is also already computed (line 45-46).

Add the variable:

```rust
// Add before the quiet move scoring section (around line 64), alongside existing variables:
let is_promo = move_flags(m) == MF_PROMOTION;
```

- [ ] **Step 6: Update `handle_beta_cutoff()` to update continuation history**

In `src/search/move_ordering.rs`, in `handle_beta_cutoff()`, after the butterfly history update and before the counter-move update, add cont1 update:

```rust
// In handle_beta_cutoff(), after add_history_score() for butterfly (line 199)
// and before the counter-move update (line 200):
// Update continuation history 1-ply
if ply > 0 && ply < 128 {
    if let Some((pp, pto)) = ctx.stack[ply - 1].cont_entry {
        let pt = piece_type(moving_piece) as usize;
        let to = move_to(m) as usize;
        let old = ctx.cont1[pp][pto][pt][to];
        // Gravity formula for cont history — same GRAVITY, same bonus
        ctx.cont1[pp][pto][pt][to] = old + bonus - (old * bonus.abs()) / HISTORY_GRAVITY;
    }
}
```

And apply malus to failed quiets (in alpha_beta.rs, at line 554, add cont1 malus alongside the butterfly malus):

```rust
// After the existing butterfly malus at line 554-555:
// Apply malus to continuation history for failed quiet moves
if ply > 0 && ply < 128 {
    if let Some((pp, pto)) = ctx.stack[ply - 1].cont_entry {
        let pt = piece_type(moving_piece) as usize;
        let to = move_to(m) as usize;
        let old = ctx.cont1[pp][pto][pt][to];
        let malus = history_malus(depth);
        ctx.cont1[pp][pto][pt][to] = old + malus - (old * malus.abs()) / HISTORY_GRAVITY;
    }
}
```

- [ ] **Step 7: Add probe event for continuation history diagnostics**

In `src/probe/events.rs`, add the event struct (append before the closing of the file):

```rust
// ============================================================
// Ch — typ:"Ch" — continuation history diagnostic
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct ChEvent {
    #[cfg_attr(feature = "probes", serde(rename = "hr"))]
    pub hit_rate: f64,             // fraction of quiet moves with non-zero cont1 score
    #[cfg_attr(feature = "probes", serde(rename = "as"))]
    pub avg_score: f64,            // average cont history contribution to move score
    #[cfg_attr(feature = "probes", serde(rename = "sa"))]
    pub saturation: f64,           // fraction of entries near ±GRAVITY
    #[cfg_attr(feature = "probes", serde(rename = "uf"))]
    pub update_freq: u64,          // updates per search
    #[cfg_attr(feature = "probes", serde(rename = "tb"))]
    pub table: &'static str,       // "cont1", "cont2", "cont4", "cont6"
}
```

In `src/probe/mod.rs`, add to the `ProbeEvent` enum:

```rust
Ch(ChEvent),
```

And add to `meta_json()` field legend:

```rust
("Ch", "cont_history: hr=hit_rate as=avg_score sa=saturation uf=update_freq tb=table"),
```

In the search, emit the probe periodically (every 10,000 nodes, or at the end of search). The simplest approach: add an `emit_cont_history_probe()` function that samples the cont1 table and emits statistics. Call it from `alpha_beta` at depth 0 (the root) after the search completes, or track stats incrementally.

For incremental tracking (preferred — avoids scanning 576 KB per probe), add to `SearchContext` or `SearchStats`:

In `src/search/stats.rs`, add fields:

```rust
pub cont1_nonzero_moves: u64,
pub cont1_total_quiet_moves: u64,
pub cont1_score_sum: i64,
pub cont1_update_count: u64,
```

Update these in the scoring and update code paths (Steps 5 and 6).

Emit the probe at the end of search (in `src/search/root.rs`, after `search()` returns the best move) using `probe!()`:

```rust
probe!(Ch, ChEvent {
    hit_rate: if stats.cont1_total_quiet_moves > 0 {
        stats.cont1_nonzero_moves as f64 / stats.cont1_total_quiet_moves as f64
    } else { 0.0 },
    avg_score: if stats.cont1_total_quiet_moves > 0 {
        stats.cont1_score_sum as f64 / stats.cont1_total_quiet_moves as f64
    } else { 0.0 },
    saturation: 0.0, // computed from sampled scan
    update_freq: stats.cont1_update_count,
    table: "cont1",
});
```

- [ ] **Step 8: Write tests for continuation history**

Add to `src/search/move_ordering_tests.rs`:

```rust
#[test]
pub(in crate::search) fn cont_history_1ply_updates_on_beta_cutoff() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let mut board = Board::startpos();

    // Make a move at ply 0 and set cont_entry for the child
    let white_move = generated_move(&board, &atk, "e2e4");
    let white_piece = board.sq_piece[move_from(white_move) as usize];
    let white_pt = piece_type(white_piece) as usize;
    let white_to = move_to(white_move) as usize;
    ctx.stack[0].cont_entry = Some((white_pt, white_to));

    let undo = board.make_move(white_move, &z);
    // Now at ply 1 (Black's turn)
    let black_move = generated_move(&board, &atk, "e7e5");
    let black_piece = board.sq_piece[move_from(black_move) as usize];
    let black_pt = piece_type(black_piece) as usize;
    let black_to = move_to(black_move) as usize;

    // Simulate beta cutoff at ply 1 — should update cont1[white_pt][white_to][black_pt][black_to]
    handle_beta_cutoff(
        &mut ctx, &board, black_move, 1, 6, false,
        100, 50, // best_score=100, beta=50
    );

    let entry = ctx.cont1[white_pt][white_to][black_pt][black_to];
    assert!(entry > -552, "cont history should increase from -552 on beta cutoff, got {entry}");

    board.unmake_move(white_move, &undo, &z);
}

#[test]
pub(in crate::search) fn cont_history_1ply_is_read_in_move_ordering() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let mut board = Board::startpos();

    // Exercise 1: no cont_entry → no score contribution
    assert_eq!(ctx.stack[0].cont_entry, None);
    // Just verify the table is initialized correctly
    assert_eq!(ctx.cont1[0][0][0][0], -552);
    // Verify nonzero initialization
    for pp in 0..6 {
        for pto in 0..64 {
            for pt in 0..6 {
                for to in 0..64 {
                    assert_eq!(ctx.cont1[pp][pto][pt][to], -552);
                }
            }
        }
    }
}

#[test]
pub(in crate::search) fn cont_history_initialized_to_negative_bias() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);

    assert_eq!(ctx.cont1[0][0][0][0], -552);
    assert_eq!(ctx.cont1[3][40][1][20], -552);
}
```

- [ ] **Step 9: Build and run tests**

Run: `cargo test cont_history -- --nocapture 2>&1 | tail -30`
Expected: All cont history tests PASS

Run: `cargo test --quiet 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 10: Build release with probes and verify**

Run: `cargo build --release --features probes 2>&1 | tail -5`
Expected: Compilation succeeds.

Run: `cargo test --quiet 2>&1 | tail -5`
Expected: Full test suite passes.

- [ ] **Step 11: Commit**

```bash
git add src/search/context.rs src/search/move_ordering.rs src/search/alpha_beta.rs \
        src/search/move_ordering_tests.rs src/search/stats.rs \
        src/probe/events.rs src/probe/mod.rs src/search/test_utils.rs
git commit -m "feat: add continuation history (1-ply)

Add cont1 table indexed by [prev_piece][prev_to][piece][to] with 576 KB
footprint, initialized to -552. Children read the previous move's piece
and destination from ctx.stack[ply-1].cont_entry to look up continuation
history scores for quiet moves.

Propagation: parent sets cont_entry after make_move(), child reads it in
score_single_move(). Updates use the gravity formula on beta cutoff for
the best move, and malus for quiet moves that failed to beat alpha.

Every top engine has continuation history. Expected +15 to +30 Elo at 1-ply.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Continuation History (2-ply)

**Files:**
- Modify: `src/search/context.rs` — add `cont2` table, add `cont_entry2` to `PlyInfo`
- Modify: `src/search/move_ordering.rs` — read cont2 score with 0.7× weight, update cont2 with half bonus
- Modify: `src/search/alpha_beta.rs` — set `cont_entry2` propagation (copy from ply-2)

**Interfaces:**
- Consumes: `cont1`, `cont_entry` from Task 2
- Consumes: `handle_beta_cutoff` with `best_score` and `beta` from Task 1
- Produces: `SearchContext.cont2: Box<[[[[i32; 64]; 6]; 64]; 6]>` — initialized to -552
- Produces: `PlyInfo.cont_entry2: Option<(usize, usize)>` — `cont_entry` from ply-2 (grandparent)

- [ ] **Step 1: Add `cont2` table and `cont_entry2` to PlyInfo**

In `src/search/context.rs`, add `cont2` after `cont1`:

```rust
// Continuation history 2-ply: [prev2_piece][prev2_to][piece][to] -> i32
pub cont2: Box<[[[[i32; 64]; 6]; 64]; 6]>,
```

In `PlyInfo`:

```rust
/// cont_entry from ply-2 (grandparent's move). Used by continuation history 2-ply.
pub cont_entry2: Option<(usize, usize)>,
```

In `SearchContext::new()`, add:

```rust
cont2: Box::new([[[[-552i32; 64]; 6]; 64]; 6]),
```

In `test_context()` (test_utils.rs), add the same initialization.

- [ ] **Step 2: Propagate `cont_entry2` in alpha_beta**

In `src/search/alpha_beta.rs`, at the same location where `cont_entry` is set for the current ply (Task 2, Step 4), also propagate the grandparent's entry:

```rust
// Propagate cont_entry2 for 2-ply continuation history:
// The child's cont_entry2 should be the current ply's parent's cont_entry.
// In other words: child reads from ply-2, which is the current ply's parent's parent.
if ply >= 2 && ply - 2 < MAX_PLY {
    ctx.stack[ply].cont_entry2 = ctx.stack[ply - 2].cont_entry;
}
```

Save and restore pattern:

```rust
let saved_cont_entry2 = ctx.stack[ply].cont_entry2;
ctx.stack[ply].cont_entry2 = /* ... */;
// ... recursive call ...
ctx.stack[ply].cont_entry2 = saved_cont_entry2;
```

- [ ] **Step 3: Add cont2 scoring with 0.7× weight**

In `src/search/move_ordering.rs`, in `score_single_move()`, add after the cont1 lookup:

```rust
// Continuation history 2-ply: 0.7× weight relative to offset 1
if ply >= 2 && ply < 128 && m != tt_move && !is_capture && !is_promo {
    if let Some((pp2, pto2)) = ctx.stack[ply - 2].cont_entry2 {
        let mover_pt = piece_type(mover) as usize;
        let to_idx = move_to(m) as usize;
        // 0.7x weight: multiply then divide
        s += (ctx.cont2[pp2][pto2][mover_pt][to_idx] * 7) / 10;
    }
}
```

- [ ] **Step 4: Update `handle_beta_cutoff()` for cont2**

In `handle_beta_cutoff()`, after the cont1 update, add cont2 update with half bonus:

```rust
// Update continuation history 2-ply with half bonus
if ply >= 2 && ply < 128 {
    if let Some((pp2, pto2)) = ctx.stack[ply - 2].cont_entry2 {
        let pt = piece_type(moving_piece) as usize;
        let to = move_to(m) as usize;
        let old = ctx.cont2[pp2][pto2][pt][to];
        let half_bonus = bonus / 2;
        ctx.cont2[pp2][pto2][pt][to] = old + half_bonus
            - (old * half_bonus.abs()) / HISTORY_GRAVITY;
    }
}
```

And in alpha_beta.rs, add cont2 malus for failed quiets alongside the cont1 malus (same location, Task 2 Step 6):

```rust
// Cont2 malus with half magnitude
if ply >= 2 && ply < 128 {
    if let Some((pp2, pto2)) = ctx.stack[ply - 2].cont_entry2 {
        let pt = piece_type(moving_piece) as usize;
        let to = move_to(m) as usize;
        let old = ctx.cont2[pp2][pto2][pt][to];
        let half_malus = history_malus(depth) / 2;
        ctx.cont2[pp2][pto2][pt][to] = old + half_malus
            - (old * half_malus.abs()) / HISTORY_GRAVITY;
    }
}
```

- [ ] **Step 5: Write tests for cont2**

Add to `src/search/move_ordering_tests.rs`:

```rust
#[test]
pub(in crate::search) fn cont_history_2ply_updates_with_half_bonus() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let mut board = Board::startpos();

    // Set cont_entry at ply 0 (will become cont_entry2 at ply 2)
    ctx.stack[0].cont_entry = Some((PieceType::Pawn as usize, 28)); // e2-e4
    ctx.stack[0].cont_entry2 = None; // no grandparent at ply 0

    let wm = generated_move(&board, &atk, "e2e4");
    let undo0 = board.make_move(wm, &z);
    ctx.stack[1].cont_entry = Some((PieceType::Pawn as usize, 28));
    ctx.stack[1].cont_entry2 = ctx.stack[0].cont_entry; // propagate

    let bm = generated_move(&board, &atk, "e7e5");
    let undo1 = board.make_move(bm, &z);
    ctx.stack[2].cont_entry = Some((PieceType::Pawn as usize, 36));
    ctx.stack[2].cont_entry2 = ctx.stack[1].cont_entry; // propagate: should be (Pawn, 28)

    // Now at ply 2, make another move and cause cutoff
    let wm2 = generated_move(&board, &atk, "g1f3");
    let wm2_pt = piece_type(board.sq_piece[move_from(wm2) as usize]) as usize;
    let wm2_to = move_to(wm2) as usize;

    handle_beta_cutoff(
        &mut ctx, &board, wm2, 2, 6, false,
        100, 50,
    );

    // cont2 should be updated: cont2[Pawn][28][Knight][f3]
    let pp = PieceType::Pawn as usize;
    let entry = ctx.cont2[pp][28][wm2_pt][wm2_to];
    assert!(entry > -552, "cont2 should increase from -552 on beta cutoff, got {entry}");

    board.unmake_move(bm, &undo1, &z);
    board.unmake_move(wm, &undo0, &z);
}
```

- [ ] **Step 6: Build and run tests**

Run: `cargo test cont_history_2ply -- --nocapture 2>&1 | tail -15`
Expected: PASS

Run: `cargo test --quiet 2>&1 | tail -10`
Expected: All tests pass.

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compilation succeeds.

- [ ] **Step 7: Commit**

```bash
git add src/search/context.rs src/search/move_ordering.rs src/search/alpha_beta.rs \
        src/search/move_ordering_tests.rs src/search/test_utils.rs
git commit -m "feat: add continuation history (2-ply)

Add cont2 table indexed by the move two plies ago. Scoring weight is
0.7x relative to 1-ply (weaker correlation at longer distance). Updates
use half bonus/malus magnitude. Propagation through cont_entry2 in
PlyInfo copies the grandparent's cont_entry.

Expected additional +5 to +15 Elo on top of 1-ply continuation history.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: Continuation History (4,6-ply)

**Files:**
- Modify: `src/search/context.rs` — add `cont4`, `cont6` tables; add `cont_entry4`, `cont_entry6` to `PlyInfo`
- Modify: `src/search/move_ordering.rs` — read cont4/cont6 with lower weights, update with quarter bonus
- Modify: `src/search/alpha_beta.rs` — propagate `cont_entry4`, `cont_entry6`

**Interfaces:**
- Consumes: `cont1`, `cont2`, propagation pattern from Tasks 2-3
- Produces: `SearchContext.cont4`, `SearchContext.cont6` — heap-allocated tables
- Produces: `PlyInfo.cont_entry4`, `PlyInfo.cont_entry6`

- [ ] **Step 1: Add cont4, cont6 tables and PlyInfo fields**

In `src/search/context.rs`, add to `SearchContext`:

```rust
// Continuation history 4-ply and 6-ply
pub cont4: Box<[[[[i32; 64]; 6]; 64]; 6]>,
pub cont6: Box<[[[[i32; 64]; 6]; 64]; 6]>,
```

In `PlyInfo`:

```rust
pub cont_entry4: Option<(usize, usize)>,
pub cont_entry6: Option<(usize, usize)>,
```

In `SearchContext::new()`:

```rust
cont4: Box::new([[[[-552i32; 64]; 6]; 64]; 6]),
cont6: Box::new([[[[-552i32; 64]; 6]; 64]; 6]),
```

Update `test_context()` in `src/search/test_utils.rs` identically.

- [ ] **Step 2: Propagate cont_entry4 and cont_entry6**

In `src/search/alpha_beta.rs`, alongside the existing propagation for cont1 and cont2:

```rust
// Propagate cont_entry4 (from ply-4)
if ply >= 4 && ply < MAX_PLY {
    ctx.stack[ply].cont_entry4 = ctx.stack[ply - 4].cont_entry;
}

// Propagate cont_entry6 (from ply-6)
if ply >= 6 && ply < MAX_PLY {
    ctx.stack[ply].cont_entry6 = ctx.stack[ply - 6].cont_entry;
}
```

- [ ] **Step 3: Add cont4 and cont6 scoring**

In `score_single_move()`, after the cont2 lookup:

```rust
// Continuation history offset 4: quarter weight, score/4
if ply >= 4 && ply < 128 && m != tt_move && !is_capture && !is_promo {
    if let Some((pp4, pto4)) = ctx.stack[ply - 4].cont_entry4 {
        let mover_pt = piece_type(mover) as usize;
        let to_idx = move_to(m) as usize;
        s += ctx.cont4[pp4][pto4][mover_pt][to_idx] / 4;
    }
}

// Continuation history offset 6: quarter weight, score/4
if ply >= 6 && ply < 128 && m != tt_move && !is_capture && !is_promo {
    if let Some((pp6, pto6)) = ctx.stack[ply - 6].cont_entry6 {
        let mover_pt = piece_type(mover) as usize;
        let to_idx = move_to(m) as usize;
        s += ctx.cont6[pp6][pto6][mover_pt][to_idx] / 4;
    }
}
```

- [ ] **Step 4: Update `handle_beta_cutoff()` for cont4 and cont6**

In `handle_beta_cutoff()`, after cont2 updates, add quarter-bonus updates for cont4 and cont6:

```rust
// Update continuation history 4-ply with quarter bonus
if ply >= 4 && ply < 128 {
    if let Some((pp4, pto4)) = ctx.stack[ply - 4].cont_entry4 {
        let pt = piece_type(moving_piece) as usize;
        let to = move_to(m) as usize;
        let old = ctx.cont4[pp4][pto4][pt][to];
        let quarter_bonus = bonus / 4;
        ctx.cont4[pp4][pto4][pt][to] = old + quarter_bonus
            - (old * quarter_bonus.abs()) / HISTORY_GRAVITY;
    }
}

// Update continuation history 6-ply with quarter bonus
if ply >= 6 && ply < 128 {
    if let Some((pp6, pto6)) = ctx.stack[ply - 6].cont_entry6 {
        let pt = piece_type(moving_piece) as usize;
        let to = move_to(m) as usize;
        let old = ctx.cont6[pp6][pto6][pt][to];
        let quarter_bonus = bonus / 4;
        ctx.cont6[pp6][pto6][pt][to] = old + quarter_bonus
            - (old * quarter_bonus.abs()) / HISTORY_GRAVITY;
    }
}
```

And quarter-malus for failed quiets in alpha_beta.rs (same pattern, using `history_malus(depth) / 4`).

- [ ] **Step 5: Write tests for cont4/cont6**

Add to `src/search/move_ordering_tests.rs`:

```rust
#[test]
pub(in crate::search) fn cont_history_4ply_and_6ply_tables_exist() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);

    // All tables initialized to -552
    assert_eq!(ctx.cont4[0][0][0][0], -552);
    assert_eq!(ctx.cont6[0][0][0][0], -552);
}

#[test]
pub(in crate::search) fn cont_history_4ply_propagation() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);

    ctx.stack[0].cont_entry = Some((PieceType::Pawn as usize, 28)); // e4

    let mut board = Board::startpos();
    let wm = generated_move(&board, &atk, "e2e4");
    let u0 = board.make_move(wm, &z);
    ctx.stack[1].cont_entry = Some((PieceType::Pawn as usize, 36)); // e5
    let bm = generated_move(&board, &atk, "e7e5");
    let u1 = board.make_move(bm, &z);
    ctx.stack[2].cont_entry = Some((PieceType::Knight as usize, 37)); // Nf3
    let wm2 = generated_move(&board, &atk, "g1f3");
    let u2 = board.make_move(wm2, &z);
    ctx.stack[3].cont_entry = Some((PieceType::Knight as usize, 21)); // Nc6
    let bm2 = generated_move(&board, &atk, "b8c6");
    let u3 = board.make_move(bm2, &z);

    // Propagate cont_entry4: from ply 0 (e4) to ply 4
    ctx.stack[4].cont_entry4 = ctx.stack[0].cont_entry;
    assert_eq!(ctx.stack[4].cont_entry4, Some((PieceType::Pawn as usize, 28)));

    board.unmake_move(bm2, &u3, &z);
    board.unmake_move(wm2, &u2, &z);
    board.unmake_move(bm, &u1, &z);
    board.unmake_move(wm, &u0, &z);
}
```

- [ ] **Step 6: Build and run tests**

Run: `cargo test cont_history_4ply cont_history_6ply -- --nocapture 2>&1 | tail -15`
Expected: PASS

Run: `cargo test --quiet 2>&1 | tail -10`
Expected: All tests pass.

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compilation succeeds.

- [ ] **Step 7: Commit**

```bash
git add src/search/context.rs src/search/move_ordering.rs src/search/alpha_beta.rs \
        src/search/move_ordering_tests.rs src/search/test_utils.rs
git commit -m "feat: add continuation history (4-ply and 6-ply)

Add cont4 and cont6 tables with quarter-weight scoring and quarter-bonus
updates. Offsets 3 and 5 are skipped (following Reckless/PlentyChess/
Obsidian/Berserk convention). The quarter weight reflects the weaker
causal link at longer move distances.

Expected additional +5 to +10 Elo combined for offsets 4 and 6.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 5: Pawn History

**Files:**
- Modify: `src/board/state.rs` — add `pawn_hash: u64` field to `Board`
- Modify: `src/board/zobrist.rs` — no changes needed (reuse existing piece_sq keys for pawn-only hash)
- Modify: `src/board/mod.rs` — add `pub fn compute_pawn_hash(board: &Board, z: &Zobrist) -> u64`
- Modify: `src/search/context.rs` — add `pawn_history` table
- Modify: `src/search/move_ordering.rs` — read pawn history in `score_single_move()`, update in `handle_beta_cutoff()`
- Modify: `src/search/alpha_beta.rs` — ensure pawn_hash is computed/available

**Interfaces:**
- Consumes: `Zobrist.piece_sq` for pawn-only hash computation
- Consumes: `handle_beta_cutoff` with bonus from Task 1
- Produces: `Board.pawn_hash: u64` — zobrist hash of pawn positions only
- Produces: `fn compute_pawn_hash(board: &Board, z: &Zobrist) -> u64` — compute pawn hash on demand
- Produces: `SearchContext.pawn_history: Box<[[[i32; 64]; 6]; 1024]>` — 1024 × 6 × 64 = 393,216 entries
- Produces: Modified `score_single_move()` adding pawn history with 0.5× weight
- Produces: Modified `handle_beta_cutoff()` updating pawn history on beta cutoff

- [ ] **Step 1: Add `pawn_hash` field to `Board` and compute function**

In `src/board/state.rs`, add the field to the `Board` struct (after `hash`):

```rust
/// Zobrist hash of pawn structure only (pawns of both colors).
/// Used by pawn history table for position-type-aware move ordering.
pub pawn_hash: u64,
```

In `src/board/mod.rs`, add the compute function:

```rust
/// Compute the pawn-only Zobrist hash for the current board position.
/// XORs piece_sq keys for all pawns on the board, ignoring other pieces.
pub fn compute_pawn_hash(board: &Board, z: &Zobrist) -> u64 {
    let mut h: u64 = 0;
    let white_pawns = board.pieces[Color::White as usize][PieceType::Pawn as usize];
    let black_pawns = board.pieces[Color::Black as usize][PieceType::Pawn as usize];
    let mut bb = white_pawns;
    while bb != 0 {
        let sq = bb_lsb(bb);
        bb &= bb - 1;
        h ^= z.piece_sq[Color::White as usize][PieceType::Pawn as usize][sq as usize];
    }
    let mut bb = black_pawns;
    while bb != 0 {
        let sq = bb_lsb(bb);
        bb &= bb - 1;
        h ^= z.piece_sq[Color::Black as usize][PieceType::Pawn as usize][sq as usize];
    }
    h
}
```

- [ ] **Step 2: Compute pawn_hash in Board::startpos() and after make_move**

In `Board::startpos()`, after the existing hash computation, set:

```rust
pawn_hash: compute_pawn_hash(&board, z),
```

Find the exact line with: `grep -n "fn startpos" src/board/setup.rs`

In `make_move()`, the pawn hash must be updated incrementally. Pawn moves, captures of pawns, and promotions all change the pawn hash. The simplest approach: recompute from scratch after the move (it's O(pawns) which is ≤ 16, essentially free). Or update incrementally by XOR-ing out the old pawn position(s) and XOR-ing in the new one(s).

Simpler and less error-prone: recompute `pawn_hash` at the end of `make_move()` using `compute_pawn_hash()`. Add before the return:

```rust
board.pawn_hash = compute_pawn_hash(board, z);
```

In `unmake_move()`, the pawn hash is restored from the undo struct. Add `pawn_hash: u64` to `UndoInfo`:

```rust
pub struct UndoInfo {
    pub captured: Piece,
    pub ep_sq: Square,
    pub castling: u8,
    pub halfmove: u8,
    pub hash: u64,
    pub pawn_hash: u64,
}
```

Save in `make_move()` before modifying the board:

```rust
let pawn_hash = board.pawn_hash;
```

Restore in `unmake_move()`:

```rust
board.pawn_hash = undo.pawn_hash;
```

Actually, since `Board` implements `Clone`, and `unmake_move` restores from `UndoInfo`, re-computing pawn_hash in `unmake_move` is the safest approach:

```rust
// At the bottom of unmake_move, after restoring board state:
board.pawn_hash = compute_pawn_hash(board, z);
```

This avoids storing it in UndoInfo and is correct (the board state has been fully restored at this point).

- [ ] **Step 3: Add pawn_history table to SearchContext**

In `src/search/context.rs`, after the continuation history tables:

```rust
/// Pawn history: [pawn_hash % 1024][piece_type][to_sq] -> i32
/// Keyed by pawn structure hash instead of the previous move.
/// Provides position-type-aware history: "knight-to-f3 is strong in
/// this pawn structure regardless of what the previous move was."
pub pawn_history: Box<[[[i32; 64]; 6]; 1024]>,
```

In `SearchContext::new()`:

```rust
pawn_history: Box::new([[[0i32; 64]; 6]; 1024]),
```

Pawn history initializes to 0 (not negative), since the pawn structure provides hard context that makes the zero-meaningful case rare. The spec doesn't explicitly specify the init value for pawn history, and 0 is the standard starting point for a table this sparse.

Update `test_context()` in `src/search/test_utils.rs` identically.

- [ ] **Step 4: Read pawn history in score_single_move()**

In `src/search/move_ordering.rs`, in `score_single_move()`, the function needs access to `board.pawn_hash`. Currently `score_single_move` takes `board: &Board`. Add the pawn history contribution for quiet moves:

```rust
// Pawn history: position-type-aware history with 0.5x weight
if !is_capture && !is_promo && m != tt_move {
    let pawn_idx = (board.pawn_hash & 1023) as usize;
    let mover_pt = piece_type(mover) as usize;
    let to_idx = move_to(m) as usize;
    s += ctx.pawn_history[pawn_idx][mover_pt][to_idx] / 2;
}
```

- [ ] **Step 5: Update `handle_beta_cutoff()` for pawn history**

In `handle_beta_cutoff()`, after the butterfly history update, add pawn history update:

```rust
// Update pawn history on beta cutoff
{
    let pawn_idx = (board.pawn_hash & 1023) as usize;
    let pt = piece_type(moving_piece) as usize;
    let to = move_to(m) as usize;
    let old = ctx.pawn_history[pawn_idx][pt][to];
    // Use same gravity formula, same bonus
    ctx.pawn_history[pawn_idx][pt][to] = old + bonus - (old * bonus.abs()) / HISTORY_GRAVITY;
}
```

- [ ] **Step 6: Write tests for pawn history**

Add to `src/search/move_ordering_tests.rs`:

```rust
#[test]
pub(in crate::search) fn pawn_history_updates_on_beta_cutoff() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let board = Board::startpos();

    let white_move = generated_move(&board, &atk, "e2e4");
    let w_pt = piece_type(board.sq_piece[move_from(white_move) as usize]) as usize;
    let w_to = move_to(white_move) as usize;

    let pawn_idx = (board.pawn_hash & 1023) as usize;
    assert_eq!(ctx.pawn_history[pawn_idx][w_pt][w_to], 0);

    handle_beta_cutoff(
        &mut ctx, &board, white_move, 1, 6, false,
        100, 50,
    );

    assert!(ctx.pawn_history[pawn_idx][w_pt][w_to] > 0,
        "pawn history should increase on beta cutoff");
}

#[test]
pub(in crate::search) fn pawn_hash_changes_after_pawn_move() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut board = Board::startpos();
    let hash_before = board.pawn_hash;

    let wm = generated_move(&board, &atk, "e2e4");
    let undo = board.make_move(wm, &z);
    assert_ne!(board.pawn_hash, hash_before,
        "pawn hash should change after a pawn move");

    board.unmake_move(wm, &undo, &z);
    assert_eq!(board.pawn_hash, hash_before,
        "pawn hash should be restored after unmake");
}
```

- [ ] **Step 7: Build and run tests**

Run: `cargo test pawn_history pawn_hash -- --nocapture 2>&1 | tail -20`
Expected: All pawn tests PASS

Run: `cargo test --quiet 2>&1 | tail -10`
Expected: All tests pass (including existing board tests, which now have pawn_hash).

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compilation succeeds.

- [ ] **Step 8: Commit**

```bash
git add src/board/state.rs src/board/mod.rs src/search/context.rs \
        src/search/move_ordering.rs src/search/move_ordering_tests.rs \
        src/search/test_utils.rs
git commit -m "feat: add pawn history table

Add pawn_history[pawn_hash % 1024][piece][to] keyed by pawn structure hash
instead of the previous move. Provides position-type-aware history that
captures patterns like 'knight-to-f3 is strong in this pawn structure
regardless of the previous move.'

Pawn hash computed from zobrist keys of all pawns on the board, stored
in Board.pawn_hash and updated incrementally during make/unmake. Scoring
uses 0.5x weight relative to butterfly history. 5 of 8 top engines have
pawn history. Expected +5 to +10 Elo.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6: Correction History

**Files:**
- Create: `src/search/correction.rs` — correction history tables, computation, and update functions
- Modify: `src/search/mod.rs` — add `mod correction` and `pub(in crate::search) use correction::*`
- Modify: `src/search/context.rs` — add correction tables to `SearchContext`
- Modify: `src/search/alpha_beta.rs` — apply correction before pruning, update after search returns
- Modify: `src/probe/events.rs` — add `CorrectionHistoryEvent`
- Modify: `src/probe/mod.rs` — add `Ch` variant for correction history
- Test: `src/search/move_ordering_tests.rs` — add correction history tests

**Interfaces:**
- Consumes: `evaluate()` for raw_eval (already called at top of alpha_beta)
- Consumes: `board.pawn_hash` from Task 5 for pawn correction lookup
- Consumes: `PlyInfo.cont_entry` for continuation correction lookup
- Consumes: `TT.raw_eval` from Layer 0 for correction-aware TT probe behavior
- Produces: `fn compute_correction(ctx, board, ply) -> i32` — returns the correction value
- Produces: `fn update_correction(ctx, board, depth, best_score, raw_eval, ply)` — updates all correction tables
- Produces: `SearchContext.pawn_corr`, `nonpawn_corr_w`, `nonpawn_corr_b`, `cont_corr` — per-thread correction tables
- Produces: `CorrectionHistoryEvent` probe with histogram, avg, rms, correlation fields

- [ ] **Step 1: Create `src/search/correction.rs` with tables and functions**

Create the new file:

```rust
use super::*;
use crate::board::{Board, Zobrist};
use crate::types::*;

// ---- Correction history constants ----

/// Maximum absolute correction value stored in any table entry.
const CORRHIST_LIMIT: i32 = 1024;

/// Divisor for correction value before applying to eval.
/// correction_value / CORRHIST_DIVISOR is the actual centipawn adjustment.
const CORRHIST_DIVISOR: i32 = 512;

/// Gravity constant for correction history updates (separate from HISTORY_GRAVITY
/// because correction values are in a different range).
const CORRHIST_GRAVITY: i32 = 1024;

/// Number of pawn correction buckets.
const PAWN_CORR_SIZE: usize = 16384;

/// Number of non-pawn correction buckets.
const NONPAWN_CORR_SIZE: usize = 16384;

/// Continuation correction dimension.
const CONT_CORR_SIZE: usize = 384;

/// Correction weights — starting values from top engine analysis.
/// w1 ≈ 30-53 (pawn), w2 ≈ 35-65 (non-pawn), w3 ≈ 27-76 (continuation).
/// Start conservative and tune via SPRT/texel.
const CORR_W1: i32 = 30;
const CORR_W2: i32 = 35;
const CORR_W3: i32 = 27;

// ---- Non-pawn hash computation ----

/// Compute a Zobrist hash of non-pawn pieces for a single color.
/// Used to key the non-pawn correction tables.
fn non_pawn_hash(board: &Board, z: &Zobrist, color: Color) -> u64 {
    let mut h: u64 = 0;
    let ci = color as usize;
    for pt in [
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ] {
        let mut bb = board.pieces[ci][pt as usize];
        while bb != 0 {
            let sq = bb_lsb(bb);
            bb &= bb - 1;
            h ^= z.piece_sq[ci][pt as usize][sq as usize];
        }
    }
    h
}

// ---- Correction computation ----

/// Compute the correction value for the current position.
/// Called once per node before any pruning decision.
/// The corrected eval = raw_eval + correction / CORRHIST_DIVISOR.
pub(in crate::search) fn compute_correction(
    ctx: &SearchContext,
    board: &Board,
    ply: usize,
) -> i32 {
    let stm = board.side as usize;
    let pawn_idx = (board.pawn_hash as usize) % PAWN_CORR_SIZE;

    let np_hash_w = non_pawn_hash(board, ctx.z, Color::White);
    let np_hash_b = non_pawn_hash(board, ctx.z, Color::Black);
    let np_idx_w = (np_hash_w as usize) % NONPAWN_CORR_SIZE;
    let np_idx_b = (np_hash_b as usize) % NONPAWN_CORR_SIZE;

    let mut corr = CORR_W1 * ctx.pawn_corr[stm][pawn_idx]
        + CORR_W2 * ctx.nonpawn_corr_w[stm][np_idx_w]
        + CORR_W2 * ctx.nonpawn_corr_b[stm][np_idx_b];

    if ply >= 2 {
        if let (Some(prev1), Some(prev2)) = (
            ctx.stack[ply - 1].cont_entry,
            ctx.stack[ply - 2].cont_entry,
        ) {
            let cont_idx = prev1.0 * 64 + prev1.1;
            let cont2_idx = prev2.0 * 64 + prev2.1;
            if cont_idx < CONT_CORR_SIZE && cont2_idx < CONT_CORR_SIZE {
                corr += CORR_W3 * ctx.cont_corr[stm][cont_idx][cont2_idx];
            }
        }
    }

    if ply >= 4 {
        if let Some(prev4) = ctx.stack[ply - 4].cont_entry {
            let cont_idx = prev4.0 * 64 + prev4.1;
            if let Some(prev2) = ctx.stack[ply - 2].cont_entry {
                let cont2_idx = prev2.0 * 64 + prev2.1;
                if cont_idx < CONT_CORR_SIZE && cont2_idx < CONT_CORR_SIZE {
                    corr += CORR_W3 * ctx.cont_corr[stm][cont_idx][cont2_idx];
                }
            }
        }
    }

    corr
}

/// Apply correction to raw_eval and return the corrected eval.
/// The raw_eval is the uncorrected static evaluation.
/// The corrected eval is what feeds into all pruning margins.
pub(in crate::search) fn corrected_eval(
    ctx: &SearchContext,
    board: &Board,
    raw_eval: Score,
    ply: usize,
) -> Score {
    let corr = compute_correction(ctx, board, ply);
    raw_eval + corr / CORRHIST_DIVISOR
}

// ---- Correction history update ----

/// Update all correction history tables after search returns from a node.
/// Uses the difference between the search result and the raw_eval to learn
/// systematic eval biases for this position type.
///
/// Must be called ONCE per node, AFTER the search returns best_score.
/// The update uses best_score - raw_eval (NOT best_score - corrected_eval) —
/// the correction learns the total eval error, not the residual after correction.
pub(in crate::search) fn update_correction(
    ctx: &mut SearchContext,
    board: &Board,
    depth: i32,
    best_score: Score,
    raw_eval: Score,
    ply: usize,
) {
    let diff = best_score - raw_eval;
    if diff.abs() < 5 {
        return; // negligible error, skip update to avoid noise
    }

    let bonus = (diff * depth / 4).clamp(
        -CORRHIST_LIMIT / 4,
        CORRHIST_LIMIT / 4,
    );

    let stm = board.side as usize;
    let pawn_idx = (board.pawn_hash as usize) % PAWN_CORR_SIZE;

    // Pawn correction
    {
        let old = ctx.pawn_corr[stm][pawn_idx];
        ctx.pawn_corr[stm][pawn_idx] = old + bonus
            - (old * bonus.abs()) / CORRHIST_GRAVITY;
    }

    // Non-pawn correction (both colors)
    let np_hash_w = non_pawn_hash(board, ctx.z, Color::White);
    let np_hash_b = non_pawn_hash(board, ctx.z, Color::Black);
    let np_idx_w = (np_hash_w as usize) % NONPAWN_CORR_SIZE;
    let np_idx_b = (np_hash_b as usize) % NONPAWN_CORR_SIZE;

    {
        let old = ctx.nonpawn_corr_w[stm][np_idx_w];
        ctx.nonpawn_corr_w[stm][np_idx_w] = old + bonus
            - (old * bonus.abs()) / CORRHIST_GRAVITY;
    }
    {
        let old = ctx.nonpawn_corr_b[stm][np_idx_b];
        ctx.nonpawn_corr_b[stm][np_idx_b] = old + bonus
            - (old * bonus.abs()) / CORRHIST_GRAVITY;
    }

    // Continuation correction (if enough history)
    if ply >= 2 {
        if let (Some(prev1), Some(prev2)) = (
            ctx.stack[ply - 1].cont_entry,
            ctx.stack[ply - 2].cont_entry,
        ) {
            let cont_idx = prev1.0 * 64 + prev1.1;
            let cont2_idx = prev2.0 * 64 + prev2.1;
            if cont_idx < CONT_CORR_SIZE && cont2_idx < CONT_CORR_SIZE {
                let old = ctx.cont_corr[stm][cont_idx][cont2_idx];
                ctx.cont_corr[stm][cont_idx][cont2_idx] = old + bonus
                    - (old * bonus.abs()) / CORRHIST_GRAVITY;
            }
        }
    }
}
```

- [ ] **Step 2: Add correction tables to SearchContext**

In `src/search/context.rs`, add fields after `pawn_history`:

```rust
/// Correction history tables — per-thread online statistical correction to
/// static eval. Learn systematic eval biases for specific position types.
pub pawn_corr: Box<[[i32; 16384]; 2]>,       // [stm][pawn_hash % 16384]
pub nonpawn_corr_w: Box<[[i32; 16384]; 2]>,   // [stm][non_pawn_hash(White) % 16384]
pub nonpawn_corr_b: Box<[[i32; 16384]; 2]>,   // [stm][non_pawn_hash(Black) % 16384]
pub cont_corr: Box<[[[i32; 384]; 384]; 2]>,   // [stm][prev_piece_to][prev2_piece_to]
```

In `SearchContext::new()`, initialize all to zero:

```rust
pawn_corr: Box::new([[0i32; 16384]; 2]),
nonpawn_corr_w: Box::new([[0i32; 16384]; 2]),
nonpawn_corr_b: Box::new([[0i32; 16384]; 2]),
cont_corr: Box::new([[[0i32; 384]; 384]; 2]),
```

Update `test_context()` in `src/search/test_utils.rs` with the same initializations.

- [ ] **Step 3: Register the new module**

In `src/search/mod.rs`, add:

```rust
mod correction;
```

And:

```rust
pub(in crate::search) use correction::*;
```

- [ ] **Step 4: Apply correction at the top of alpha_beta**

In `src/search/alpha_beta.rs`, after computing `static_eval` (line 166-174), compute the corrected eval:

```rust
// Apply correction history to debias static_eval for pruning heuristics.
// The raw static_eval is stored in the stack for correction update after
// search returns. The corrected eval feeds into RFP, NMP, FFP, razoring,
// ProbCut, and LMR margins.
let corrected_eval = corrected_eval(ctx, board, static_eval, ply);
```

Then replace ALL uses of `static_eval` in pruning margins with `corrected_eval`:

- Line 187: `rfp_prune_score(static_eval, ...)` → `rfp_prune_score(corrected_eval, ...)`
- Line 193: `try_null_move(board, ctx, beta, depth, ply, static_eval)` → `try_null_move(board, ctx, beta, depth, ply, corrected_eval)`
- Line 240: The `prev_static_eval` for is_improving should use the raw eval (since improving compares raw scores)
- Line 299: `static_eval` in FFP → `corrected_eval`
- All other pruning uses of `static_eval` → `corrected_eval`

**Important:** `ctx.stack[ply].static_eval = Some(static_eval)` at line 177 should store the RAW eval (for correction update and is_improving). The correction is applied on top.

The key places to change:
1. `is_improving()` call at line 175: keep using `static_eval` (raw — compares actual eval trend)
2. `rfp_prune_score()` at line 187: use `corrected_eval`
3. `try_null_move()` at line 193: use `corrected_eval`
4. FFP calls: use `corrected_eval`
5. LMR adjustments: use `corrected_eval`

Use grep to find all uses of `static_eval` in alpha_beta.rs:

```bash
grep -n "static_eval" src/search/alpha_beta.rs
```

Replace each one appropriately. The rule:
- `is_improving` and `ctx.stack[ply].static_eval` → raw `static_eval`
- All pruning margin computations → `corrected_eval`

- [ ] **Step 5: Update correction history after search returns**

In `src/search/alpha_beta.rs`, after the move loop and before the TT store (around line 610), update the correction history:

```rust
// Update correction history using the search result vs raw eval.
// Uses raw_eval (the uncorrected static evaluation), NOT corrected_eval.
// The correction learns the TOTAL eval error, not the residual.
let raw_eval_for_correction = ctx.stack[ply].static_eval.unwrap_or(static_eval);
update_correction(ctx, board, depth, best_score, raw_eval_for_correction, ply);
```

Note: `static_eval` at this point is the raw eval (before correction). The variable name is `static_eval` — it was never reassigned. The `corrected_eval` was used only for the pruning margins. `ctx.stack[ply].static_eval` should also be the raw eval (set at line 177 before correction was applied).

- [ ] **Step 6: Add probe event for correction history**

In `src/probe/events.rs`, add:

```rust
// ============================================================
// Cr — typ:"Cr" — correction history diagnostic
// ============================================================
#[cfg_attr(feature = "probes", derive(Serialize))]
pub struct CorrectionHistoryEvent {
    #[cfg_attr(feature = "probes", serde(rename = "cv"))]
    pub correction_value: i32,       // total correction applied (in corr units, divide by 512 for cp)
    #[cfg_attr(feature = "probes", serde(rename = "re"))]
    pub raw_eval: Score,             // raw static eval before correction
    #[cfg_attr(feature = "probes", serde(rename = "ce"))]
    pub corrected_eval: Score,       // eval after correction
    #[cfg_attr(feature = "probes", serde(rename = "df"))]
    pub diff: Score,                 // best_score - raw_eval (the error being learned)
    #[cfg_attr(feature = "probes", serde(rename = "pc"))]
    pub pawn_corr: i32,              // pawn correction component
    #[cfg_attr(feature = "probes", serde(rename = "np"))]
    pub nonpawn_corr: i32,           // non-pawn correction component
    #[cfg_attr(feature = "probes", serde(rename = "cc"))]
    pub cont_corr: i32,              // continuation correction component
    #[cfg_attr(feature = "probes", serde(rename = "pl"))]
    pub ply: u32,
}
```

In `src/probe/mod.rs`, add:

```rust
Cr(CorrectionHistoryEvent),
```

And to `meta_json()`:

```rust
("Cr", "corr_hist: cv=corr_value re=raw_eval ce=corrected df=diff pc=pawn_corr np=nonpawn_cc=cont_corr pl=ply"),
```

Emit the probe at the point where correction is computed (Step 4):

```rust
sample_probe!(Cr, CorrectionHistoryEvent {
    correction_value: corr_val,
    raw_eval: static_eval,
    corrected_eval: corrected_eval,
    diff: 0, // not known yet at this point — updated in a later probe
    pawn_corr: pawn_corr_component,
    nonpawn_corr: nonpawn_corr_component,
    cont_corr: cont_corr_component,
    ply: ply as u32,
});
```

The component breakdown requires modifying `compute_correction()` to also return the components. Add a helper:

```rust
pub(in crate::search) fn compute_correction_components(
    ctx: &SearchContext,
    board: &Board,
    ply: usize,
) -> (i32, i32, i32, i32) {
    // Returns (total, pawn_component, nonpawn_component, cont_component)
    // ... same logic as compute_correction but returns the breakdown
}
```

Or just compute the total with `compute_correction()` and log it at the update point where `diff` is known. For the initial implementation, keep the probe simple: emit at the update point (Step 5) where all values are known.

- [ ] **Step 7: Write tests for correction history**

Add to `src/search/move_ordering_tests.rs`:

```rust
#[test]
pub(in crate::search) fn correction_history_is_zero_on_startpos() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let board = Board::startpos();

    // At the start position, correction should be zero (no history yet)
    let corr = compute_correction(&ctx, &board, 0);
    assert_eq!(corr, 0, "correction should be zero with no history");
}

#[test]
pub(in crate::search) fn correction_history_updates_after_search() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let mut ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);
    let board = Board::startpos();

    // Simulate a search returning a score that differs from raw eval
    let raw_eval = 30; // centipawns
    let best_score = 70; // search found a +70cp line (40cp better than static eval)

    update_correction(&mut ctx, &board, 6, best_score, raw_eval, 1);

    // After update, the correction should reflect the positive surprise
    let corr = compute_correction(&ctx, &board, 1);
    // The correction should be positive (eval was underestimating)
    assert!(corr > 0, "correction should be positive after positive diff, got {corr}");
}

#[test]
pub(in crate::search) fn correction_history_components_exist() {
    let atk = AttackTables::init();
    let z = Zobrist::new();
    let mut tt = TranspositionTable::new(16);
    let stop = AtomicBool::new(false);
    let ctx = test_context(&atk, &z, &mut tt, Limits::default(), &stop);

    // All correction tables should be initialized to zero
    assert_eq!(ctx.pawn_corr[0][0], 0);
    assert_eq!(ctx.nonpawn_corr_w[0][0], 0);
    assert_eq!(ctx.nonpawn_corr_b[0][0], 0);
    assert_eq!(ctx.cont_corr[0][0][0], 0);
}
```

- [ ] **Step 8: Build and run tests**

Run: `cargo test correction_history -- --nocapture 2>&1 | tail -20`
Expected: All correction history tests PASS

Run: `cargo test --quiet 2>&1 | tail -10`
Expected: All tests pass.

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compilation succeeds.

Run: `cargo build --release --features probes 2>&1 | tail -5`
Expected: Compilation with probes succeeds.

- [ ] **Step 9: Commit**

```bash
git add src/search/correction.rs src/search/mod.rs src/search/context.rs \
        src/search/alpha_beta.rs src/search/move_ordering_tests.rs \
        src/probe/events.rs src/probe/mod.rs src/search/test_utils.rs
git commit -m "feat: add correction history

Add four per-thread correction tables that learn systematic eval biases:
- pawn_corr[stm][pawn_hash % 16384] for pawn structure bias
- nonpawn_corr_w/b[stm][non_pawn_hash % 16384] for piece configuration bias
- cont_corr[stm][prev_piece_to][prev2_piece_to] for context-dependent bias

Correction is computed once per node before pruning and added to static
eval. Update uses best_score - raw_eval (total error, not residual).
Weights: w1=30, w2=35, w3=27 (starting values, tune via SPRT/texel).

6 of 8 top engines have correction history. The two without it (Ethereal,
Marvin) are the weakest engines in the set. Expected +8 to +20 Elo.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Layer 1 Complete Checklist

After all 6 tasks are implemented and committed:

- [ ] `cargo test --quiet` — full test suite passes
- [ ] `cargo build --release` — compiles without warnings
- [ ] `cargo build --release --features probes` — probe build compiles
- [ ] Task 1 SPRT: bonus formula upgrade → expected +3 to +8 Elo
- [ ] Task 2 SPRT: 1-ply continuation history → expected +15 to +30 Elo
- [ ] Task 3 SPRT: 2-ply continuation history → expected +5 to +15 Elo
- [ ] Task 4 SPRT: 4,6-ply continuation history → expected +5 to +10 Elo
- [ ] Task 5 SPRT: pawn history → expected +5 to +10 Elo
- [ ] Task 6 SPRT: correction history → expected +8 to +20 Elo
- [ ] Probe data confirms continuation history is learning (ch_hit_rate > 50%, ch_avg_score non-zero, ch_saturation < 20%)
- [ ] Probe data confirms correction history is correlated with search error (corr_search_error_correlation > 0.5)
- [ ] All existing killer moves and counter-move heuristics still active (not removed, not weight-reduced)
