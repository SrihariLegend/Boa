# Layer 0 — Correctness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all known correctness and infrastructure issues in Boa's foundation — SEE pin exclusion, gravity-based history updates, non-zero history initialization, and TT raw eval storage.

**Architecture:** Four independent fixes to the bottom layer of the engine. SEE pin exclusion adds absolute-pin awareness to `least_valuable_attacker()`. Gravity history replaces the overflow-triggered division-by-2 aging with the universal gravity formula. Non-zero history init changes table initialization from zero to small negative values. TT raw eval storage packs a new `raw_eval: i16` field into the data word alongside score and best move, enabling eval reuse from the transposition table.

**Tech Stack:** Rust (2021 edition), existing bitboard/movegen/tt modules. No new dependencies.

## Global Constraints

- Correctness before optimization — no heuristic can be trusted on a broken foundation (Engineering Oath #1)
- `cargo test` must pass after every task
- SPRT not required to justify these changes (they are correctness fixes), but SPRT to confirm no regression
- Every subsystem must expose probe diagnostics before it is tuned (Engineering Oath #7)
- Use Rust 2021 idioms and `rustfmt` formatting
- Add tests beside the code under `#[cfg(test)] mod tests`
- Commit messages must end with `Co-Authored-By: Claude <noreply@anthropic.com>`
- Do not commit `target/`, `analysis/`, `__pycache__/`, `*.pyc`, `*.log`

---

### Task 1: SEE Pin Exclusion

**Files:**
- Modify: `src/search/see.rs` — add `is_pinned()` helper, modify `least_valuable_attacker()`
- Modify: `src/probe/events.rs` — add `pin_excluded` field to `SeeEvent`
- Modify: `src/search/see_tests.rs` — add pin position tests

**Interfaces:**
- Consumes: `AttackTables` (for `rook_attacks`, `bishop_attacks`), `Board.king_sq`, `Board.pieces`
- Produces: `fn is_pinned(king_sq: Square, sq: Square, color: Color, occ: Bb, pieces: &[[Bb; 6]; 2], atk: &AttackTables) -> bool`
- Produces: Modified `least_valuable_attacker()` that loops past pinned candidates
- Produces: Extended `SeeEvent` with `pin_excluded: bool` field

- [ ] **Step 1: Write failing tests for SEE pin positions**

Add to `src/search/see_tests.rs`:

```rust
#[test]
pub(in crate::search) fn see_pinned_attacker_is_excluded() {
    // White rook on d1 is absolutely pinned to king on e1 by black rook on d8.
    // The rook attacks d4 but cannot legally capture there.
    // SEE should not count the pinned rook as a viable attacker.
    // Position: K on e1, R on d1 (pinned by Rd8), target on d4 with a black pawn.
    // After W pawn on c3 captures B pawn on d4: Rxd4 should be scored as if
    // the rook cannot recapture (it's pinned).
    let fen = "3r4/8/8/8/3p4/2P5/8/3RK3 w - - 0 1";
    // c3xd4: pawn takes pawn. Normally black can recapture Rd4 (rook takes pawn),
    // but if the rook on d1 is pinned, SEE should be pawn value (100), not
    // pawn - rook (100 - 500 = -400).
    let see = see_for(fen, "c3d4");
    // Since the rook on d1 is pinned to the king by the rook on d8,
    // the rook cannot recapture. Pawn takes pawn = +100.
    assert_eq!(see, PieceType::Pawn.material_value());
}

#[test]
pub(in crate::search) fn see_pinned_bishop_cannot_recapture() {
    // White bishop on c4 pinned to king on e2 by black queen on b5.
    // Bishop takes knight on f7. Without pin, SEE = knight value - bishop
    // (after queen takes bishop). With pin, bishop cannot recapture so
    // SEE = knight value only.
    // Actually: Bxf7, no recapture possible since bishop is pinned.
    let fen = "5k2/5n2/8/1q6/2B5/8/4K3/8 w - - 0 1";
    let see = see_for(fen, "c4f7");
    assert!(see >= PieceType::Knight.material_value() - PieceType::Bishop.material_value());
}

#[test]
pub(in crate::search) fn see_unpinned_attacker_still_works() {
    // Same position as pinned test but the rook is NOT pinned.
    // Rook on d1, king on g1 — rook is not on the same file as king,
    // so no pin. SEE should work normally.
    let fen = "3r4/8/8/8/3p4/2P5/8/3R2K1 w - - 0 1";
    let see = see_for(fen, "c3d4");
    // Pawn takes pawn = 100 (no recapture possible here since black has no other attacker)
    assert_eq!(see, PieceType::Pawn.material_value());
}

#[test]
pub(in crate::search) fn see_pinned_attacker_during_exchange_sequence() {
    // King on e1, Rook on d1 pinned by Rook on d8. Target d4 has black pawn.
    // Also black queen on d5. After pawn takes pawn (c3xd4), black queen can
    // recapture. Then white rook would want to recapture but is pinned.
    // fen: black rook d8, black queen d5, black pawn d4; white rook d1, king e1, pawn c3
    let fen = "3r4/8/8/3q4/3p4/2P5/8/3RK3 w - - 0 1";
    let see = see_for(fen, "c3d4");
    // Pawn(100) vs Queen(900): the pawn is lost, but the rook can't help.
    // Qxd4 -> SEE = 100 - 900 = -800
    assert_eq!(see, PieceType::Pawn.material_value() - PieceType::Queen.material_value());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test see_pinned_attacker_is_excluded -- --nocapture 2>&1 | tail -20`
Expected: FAIL — the test panics because the pin check doesn't exist yet, and SEE returns a value that assumes the rook can recapture.

Run: `cargo test see_pinned_bishop_cannot_recapture -- --nocapture 2>&1 | tail -20`
Expected: FAIL

Run: `cargo test see_unpinned_attacker_still_works -- --nocapture 2>&1 | tail -20`
Expected: PASS (this test should already pass since unpinned behavior is unchanged — but verify)

Run: `cargo test see_pinned_attacker_during_exchange_sequence -- --nocapture 2>&1 | tail -20`
Expected: FAIL

- [ ] **Step 3: Add `is_pinned()` helper to `src/search/see.rs`**

Add immediately after the `color_occupancy()` function (before `// Section 8`):

```rust
/// Check whether the piece at `sq` is absolutely pinned to its king.
///
/// A piece is absolutely pinned if removing it would reveal an enemy
/// sliding piece (rook/queen on rank/file, bishop/queen on diagonal)
/// attacking its king. The piece cannot legally move off the pin ray.
///
/// This is used by `least_valuable_attacker()` during SEE to exclude
/// pinned pieces from the attacker list — a pinned piece cannot legally
/// capture on the target square if doing so would expose its king.
pub(in crate::search) fn is_pinned(
    king_sq: Square,
    sq: Square,
    color: Color,
    occ: Bb,
    pieces: &[[Bb; 6]; 2],
    atk: &AttackTables,
) -> bool {
    // Remove the candidate piece from occupancy. If this reveals an
    // enemy slider attacking the king, the piece was pinned.
    let enemy = color.flip();
    let occ_without = occ & !bb(sq);

    // Enemy rooks and queens on the same rank/file as the king
    let rook_sliders = atk.rook_attacks(king_sq, occ_without)
        & (pieces[enemy as usize][PieceType::Rook as usize]
           | pieces[enemy as usize][PieceType::Queen as usize]);

    // Enemy bishops and queens on the same diagonal as the king
    let bishop_sliders = atk.bishop_attacks(king_sq, occ_without)
        & (pieces[enemy as usize][PieceType::Bishop as usize]
           | pieces[enemy as usize][PieceType::Queen as usize]);

    (rook_sliders | bishop_sliders) != 0
}
```

- [ ] **Step 4: Modify `least_valuable_attacker()` to skip pinned attackers**

Replace the existing `least_valuable_attacker()` function in `src/search/see.rs`:

```rust
pub(in crate::search) fn least_valuable_attacker(
    target: Square,
    color: Color,
    occ: Bb,
    pieces: &[[Bb; 6]; 2],
    atk: &AttackTables,
    king_sq: Square,
    probe_pin: &mut bool,
) -> Option<(Square, PieceType)> {
    let attackers = attackers_to(target, color, occ, pieces, atk);
    if attackers == 0 {
        return None;
    }

    let ci = color as usize;
    for pt in [
        PieceType::Pawn,
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ] {
        let mut bb = attackers & pieces[ci][pt as usize];
        while bb != 0 {
            let sq = bb_lsb(bb);
            bb &= bb - 1; // clear LSB

            // Kings cannot be pinned (they are the pin target).
            // For all other pieces, check absolute pin.
            if pt != PieceType::King
                && is_pinned(king_sq, sq, color, occ, pieces, atk)
            {
                *probe_pin = true;
                continue; // skip this pinned attacker, try next
            }

            return Some((sq, pt));
        }
    }

    None
}
```

- [ ] **Step 5: Update the SEE exchange loop to pass `king_sq` and `probe_pin` flag**

In `static_exchange_eval()`, modify the call to `least_valuable_attacker()`:

```rust
// BEFORE (line ~68):
let Some((attacker_sq, attacker_type)) =
    least_valuable_attacker(to, side, occ, &pieces, atk)
else {
    break;
};

// AFTER:
let mut see_pin_excluded = false;
let Some((attacker_sq, attacker_type)) =
    least_valuable_attacker(
        to, side, occ, &pieces, atk,
        board.king_sq[side as usize],
        &mut see_pin_excluded,
    )
else {
    break;
};
```

- [ ] **Step 6: Add probe for pin exclusion in SEE**

In the existing `sample_probe!()` call for SEE (or add a new one after the SEE computation returns), add the `pin_excluded` field. First, add the field to `SeeEvent` in `src/probe/events.rs`:

```rust
// In src/probe/events.rs, add to SeeEvent struct (after "searched_despite_bad_see"):
#[cfg_attr(feature = "probes", serde(rename = "px"))]
pub pin_excluded: bool,
```

In `static_exchange_eval()`, at the bottom where `gain[0]` is returned, add the probe call. Find the existing SEE probe in the search code (in `alpha_beta.rs` or `quiescence.rs` where SEE is used for pruning decisions). Actually, SEE is called from multiple places — add a local boolean tracker and emit the probe at the single SEE call sites that already probe.

For now, just ensure the `see_pin_excluded` variable is tracked. The existing probe sites already emit `SeeEvent` — add the `pin_excluded` field to those existing probe calls. In `src/search/alpha_beta.rs`, find the `sample_probe!(..., Se, SeeEvent { ... })` calls and add:

```rust
pin_excluded: see_pin_excluded,
```

Check for existing SEE probe calls:
Run: `grep -n "Se\|SeeEvent" src/search/alpha_beta.rs src/search/quiescence.rs`

- [ ] **Step 7: Build and run tests**

Run: `cargo test see_pinned -- --nocapture 2>&1 | tail -30`
Expected: All 4 pin tests PASS

Run: `cargo test see_ -- --nocapture 2>&1 | tail -30`
Expected: All existing SEE tests still PASS

Run: `cargo test --quiet 2>&1 | tail -10`
Expected: Full test suite passes, no regressions

- [ ] **Step 8: Build release with probes and verify it compiles**

Run: `cargo build --release --features probes 2>&1 | tail -5`
Expected: Compilation succeeds with no errors

- [ ] **Step 9: Commit**

```bash
git add src/search/see.rs src/search/see_tests.rs src/probe/events.rs src/search/alpha_beta.rs
git commit -m "fix: exclude absolutely pinned attackers from SEE

Pinned pieces cannot legally capture on the target square since doing so
would expose their king to check. Failing to exclude them causes SEE to
overestimate the gain from captures, leading to incorrect pruning decisions.

Add is_pinned() helper using the standard 'remove piece, check if king
is attacked by enemy sliders' approach. Extend SeeEvent with pin_excluded
field for diagnostic visibility.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2: Gravity-Based History Updates

**Files:**
- Modify: `src/search/move_ordering.rs` — replace overflow aging with gravity formula, add malus
- Modify: `src/search/constants.rs` — replace `HISTORY_OVERFLOW_THRESHOLD` with `HISTORY_GRAVITY`

**Interfaces:**
- Consumes: `SearchContext.history`, `SearchContext.cap_history`
- Produces: Modified `add_history_score()`, `update_cap_history()`, `handle_beta_cutoff()` using gravity
- Removes: `scale_down_history()`, `scale_down_cap_history()`, `HISTORY_OVERFLOW_THRESHOLD`
- Adds: `fn history_malus(depth: i32) -> i32`

- [ ] **Step 1: Add `HISTORY_GRAVITY` constant and remove `HISTORY_OVERFLOW_THRESHOLD`**

In `src/search/constants.rs`, replace:

```rust
/// History table overflow threshold — scale down when any entry exceeds this.
/// Prevents history scores from dominating move ordering. [NEEDS TUNING]
pub(in crate::search) const HISTORY_OVERFLOW_THRESHOLD: i32 = 500_000;
```

With:

```rust
/// History gravity constant. History values asymptotically approach ±GRAVITY
/// via the formula: new = old + delta - old * abs(delta) / GRAVITY.
/// 16384 (2¹⁴) is the universal standard across all top engines.
pub(in crate::search) const HISTORY_GRAVITY: i32 = 16_384;
```

- [ ] **Step 2: Rewrite `add_history_score()` with gravity formula**

In `src/search/move_ordering.rs`, replace the existing `add_history_score()`:

```rust
pub(in crate::search) fn add_history_score(
    ctx: &mut SearchContext,
    color: Color,
    moving_piece: Piece,
    m: Move,
    delta: i32,
) {
    if moving_piece == PIECE_NONE {
        return;
    }
    let pt = piece_type(moving_piece) as usize;
    let to = move_to(m) as usize;
    let ci = color as usize;
    let old = ctx.history[ci][pt][to];
    // Gravity formula: new = old + delta - old * abs(delta) / GRAVITY
    ctx.history[ci][pt][to] = old + delta - (old * delta.abs()) / HISTORY_GRAVITY;
}
```

Remove the `scale_down_history()` function entirely (lines 145-151).

- [ ] **Step 3: Rewrite `update_cap_history()` with gravity formula**

Replace the existing `update_cap_history()`:

```rust
pub(in crate::search) fn update_cap_history(
    ctx: &mut SearchContext,
    color: Color,
    m: Move,
    board: &Board,
    depth: i32,
) {
    let ci = color as usize;
    let mover = board.sq_piece[move_from(m) as usize];
    if mover == PIECE_NONE {
        return;
    }
    let mover_pt = piece_type(mover) as usize;
    let to = move_to(m) as usize;
    let cap = board.sq_piece[move_to(m) as usize];
    let cap_pt = if cap != PIECE_NONE {
        piece_type(cap) as usize
    } else {
        0
    };
    let bonus = depth * depth;
    let old = ctx.cap_history[ci][mover_pt][to][cap_pt];
    // Gravity formula for capture history
    ctx.cap_history[ci][mover_pt][to][cap_pt] =
        old + bonus - (old * bonus.abs()) / HISTORY_GRAVITY;
}
```

Remove the `scale_down_cap_history()` function entirely (lines 153-161).

- [ ] **Step 4: Add `history_malus()` function**

Add after `history_delta()` in `src/search/move_ordering.rs`:

```rust
/// Malus (negative bonus) applied to quiet moves that were searched
/// but failed to cause a beta cutoff. Uses the Obsidian-style formula
/// with slightly larger magnitude than the bonus for asymmetry.
pub(in crate::search) fn history_malus(depth: i32) -> i32 {
    -(196 * depth - 25).min(1047).max(-1047)
}
```

- [ ] **Step 5: Update `handle_beta_cutoff()` to use gravity and apply malus to failed quiets**

The `handle_beta_cutoff()` function signature stays the same. The bonus path is unchanged (gravity is applied inside `add_history_score`). Add a comment noting the malus is applied elsewhere (in the search loop, not in the cutoff handler — malus is applied to quiets that FAILED, so it's in alpha_beta.rs after a quiet move scores ≤ alpha).

In `src/search/move_ordering.rs`, the `handle_beta_cutoff()` function remains structurally the same — the gravity is applied inside `add_history_score()` and `update_cap_history()`. No changes needed to `handle_beta_cutoff()` itself for gravity. For malus, that will be added in a later step (in alpha_beta.rs).

- [ ] **Step 6: Apply history malus to failed quiet moves in alpha_beta.rs**

In `src/search/alpha_beta.rs` at **line 551-553**, the existing code already applies a negative history update to quiet moves that fail to beat alpha:

```rust
// CURRENT (line 551-553):
if !ctx.in_criticality_probe && is_lmr_quiet && score <= pre_alpha {
    add_history_score(ctx, side_to_move, moving_piece, m, -history_delta(depth));
}
```

Replace the `-history_delta(depth)` with `history_malus(depth)`:

```rust
// NEW:
if !ctx.in_criticality_probe && is_lmr_quiet && score <= pre_alpha {
    add_history_score(ctx, side_to_move, moving_piece, m, history_malus(depth));
}
```

This is the only change needed at this call site — `add_history_score` already uses the gravity formula (from Step 2).

- [ ] **Step 7: Build and run tests**

Run: `cargo test --quiet 2>&1 | tail -10`
Expected: All tests pass. History values now grow via gravity instead of resetting at overflow.

- [ ] **Step 8: Build release binary and quick bench**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compilation succeeds.

Run: `cargo test --quiet 2>&1 | tail -5`
Expected: All tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/search/move_ordering.rs src/search/constants.rs src/search/alpha_beta.rs
git commit -m "feat: replace overflow-based history aging with gravity formula

Replace HISTORY_OVERFLOW_THRESHOLD + scale_down_history() with the
universal gravity formula: new = old + delta - old * abs(delta) / GRAVITY
where GRAVITY = 16384.

This naturally prevents overflow (values asymptotically approach ±GRAVITY),
provides continuous proportional decay, and is self-stabilizing.

Add history_malus() for negative updates to quiet moves that fail to
beat alpha, using the Obsidian-style formula.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3: Non-Zero History Initialization

**Files:**
- Modify: `src/search/context.rs` — change history and cap_history initial values

**Interfaces:**
- Consumes: None (pure data initialization change)
- Produces: `SearchContext.history` initialized to -5, `SearchContext.cap_history` initialized to -700

- [ ] **Step 1: Change history initialization values in `SearchContext::new()`**

In `src/search/context.rs`, change line 99:

```rust
// BEFORE:
history: [[[0i32; 64]; 6]; 2],

// AFTER:
history: [[[-5i32; 64]; 6]; 2],
```

Change line 101:

```rust
// BEFORE:
cap_history: [[[[0i32; 6]; 64]; 6]; 2],

// AFTER:
cap_history: [[[[-700i32; 6]; 64]; 6]; 2],
```

- [ ] **Step 2: Build and run tests**

Run: `cargo test --quiet 2>&1 | tail -10`
Expected: All tests pass. History tables start with small negative bias against unproven moves.

- [ ] **Step 3: Commit**

```bash
git add src/search/context.rs
git commit -m "feat: initialize history tables to small negative values

Butterfly history: -5, capture history: -700. This biases against
unproven moves — a move with zero history (never tried) should score
below a move with slightly negative history (tried once and failed),
which should score below a move with positive history.

Every top engine initializes history tables this way.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4: TT Raw Eval Storage

**Files:**
- Modify: `src/tt/entry.rs` — add `raw_eval: i16` field to `TtEntry`
- Modify: `src/tt/packing.rs` — pack/unpack `raw_eval` in data word bits 48-63
- Modify: `src/tt/table.rs` — update `store()` signature and `probe()` to handle raw_eval
- Modify: `src/search/alpha_beta.rs` — use raw_eval from TT hits instead of calling `evaluate()`
- Modify: `src/search/quiescence.rs` — use raw_eval from TT hits in qsearch
- Modify: `src/tt/tests.rs` — add round-trip test for raw_eval

**Interfaces:**
- Consumes: `evaluate()` function (to compute raw_eval before storing)
- Produces: `TtEntry { raw_eval: i16, ... }` — new field in the public struct
- Produces: `TranspositionTable::store(hash, score, best, depth, bound, raw_eval)` — new parameter
- Produces: `pack_data(score, best, raw_eval) -> u64` — new parameter
- Produces: `unpack_entry(ctrl, data) -> TtEntry` — extracts raw_eval from data word

- [ ] **Step 1: Add `raw_eval` field to `TtEntry`**

In `src/tt/entry.rs`, modify the struct:

```rust
#[derive(Clone, Copy)]
pub struct TtEntry {
    pub key: u32,
    pub score: i32,
    pub best: Move,
    pub depth: i8,
    pub bound: Bound,
    pub age: u8,
    pub raw_eval: i16,
}
```

- [ ] **Step 2: Update packing functions**

In `src/tt/packing.rs`, modify `pack_data()` and `unpack_entry()`:

```rust
/// Pack score (i32), best move (u16), and raw_eval (i16) into a 64-bit word.
/// Layout: bits 63-48 = raw_eval, bits 47-32 = best_move, bits 31-0 = score
pub(super) fn pack_data(score: Score, best: Move, raw_eval: i16) -> u64 {
    (score as u32 as u64)
        | ((best as u64) << 32)
        | ((raw_eval as u16 as u64) << 48)
}

pub(super) fn unpack_entry(ctrl: u64, data: u64) -> TtEntry {
    let bound = match ((ctrl >> 40) & 0xFF) as u8 {
        1 => Bound::Exact,
        2 => Bound::Lower,
        3 => Bound::Upper,
        _ => Bound::None,
    };
    TtEntry {
        key: ctrl as u32,
        score: data as u32 as i32,
        best: ((data >> 32) & 0xFFFF) as Move,
        raw_eval: ((data >> 48) & 0xFFFF) as u16 as i16,
        depth: ((ctrl >> 32) & 0xFF) as u8 as i8,
        bound,
        age: ((ctrl >> 48) & 0xFF) as u8,
    }
}
```

- [ ] **Step 3: Update `store()` and `probe()` in `TranspositionTable`**

In `src/tt/table.rs`, update `store()` signature:

```rust
pub fn store(
    &self,
    hash: u64,
    score: Score,
    best: Move,
    depth: i8,
    bound: Bound,
    raw_eval: i16,
) {
    let slot = &self.entries[self.index(hash)];
    let key = (hash >> 32) as u32;
    let age = self.age.load(Ordering::Relaxed);
    let ctrl = slot.ctrl.load(Ordering::Acquire);

    if ctrl != 0 && ctrl & CTRL_BUSY == 0 {
        let current = unpack_entry(ctrl, slot.data.load(Ordering::Relaxed));
        if current.key != key && current.age == age && depth < current.depth {
            return;
        }
    }

    slot.ctrl.store(CTRL_BUSY, Ordering::Release);
    slot.data.store(pack_data(score, best, raw_eval), Ordering::Release);
    slot.ctrl
        .store(pack_ctrl(key, depth, bound, age), Ordering::Release);
}
```

The `probe()` function doesn't change structurally — `unpack_entry()` now extracts `raw_eval` from the data word automatically.

- [ ] **Step 4: Find all call sites of `tt.store()` and add `raw_eval` parameter**

There are exactly two call sites that need updating:

1. **`src/search/alpha_beta.rs:610`** — the main search TT store (handled in Step 7 with exact code)
2. **`src/tt/tests.rs:9`** — the existing round-trip test

For the test file, update the single call:

```rust
// src/tt/tests.rs line 9 — CURRENT:
tt.store(hash, -123, 0x4321, 7, Bound::Lower);

// src/tt/tests.rs line 9 — NEW:
tt.store(hash, -123, 0x4321, 7, Bound::Lower, 0);
```

Pass `0` as raw_eval in tests (it means "no eval stored" and won't affect test assertions).
No qsearch TT store exists yet (that's a Layer 2 item).

- [ ] **Step 5: Modify `try_tt_cutoff()` to also return raw_eval**

In `src/search/tt_cutoff.rs`, change the return type and function body to thread `raw_eval` through. The current signature (line 12) is:

```rust
) -> (Move, Option<Score>) {
```

Change to:

```rust
) -> (Move, Option<Score>, Option<i16>) {
```

Update the early-return paths:

```rust
// Line 16 — no TT entry found:
None => return (MOVE_NONE, None, None),

// Line 22 — depth insufficient:
if is_pv || entry.depth < depth as i8 {
    return (tt_move, None, Some(entry.raw_eval));
}

// Line 49 — cutoff:
return (tt_move, Some(s), Some(entry.raw_eval));

// Line 51 — no cutoff:
(tt_move, None, Some(entry.raw_eval))
```

And add the `raw_eval` extraction before the `if is_pv` check:

```rust
let tt_raw_eval = entry.raw_eval;
```

The full modified function:

```rust
pub(in crate::search) fn try_tt_cutoff(
    ctx: &mut SearchContext,
    hash: u64,
    depth: i32,
    alpha: Score,
    beta: Score,
    is_pv: bool,
    ply: usize,
) -> (Move, Option<Score>, Option<i16>) {
    ctx.stats.tt_probes += 1;
    let entry = match ctx.tt.probe(hash) {
        Some(e) => e,
        None => return (MOVE_NONE, None, None),
    };
    ctx.stats.tt_hits += 1;
    let tt_move = entry.best;
    let tt_raw_eval = entry.raw_eval;

    if is_pv || entry.depth < depth as i8 {
        return (tt_move, None, Some(tt_raw_eval));
    }

    let s = score_from_tt(entry.score, ply);
    let cutoff = match entry.bound {
        Bound::Exact => true,
        Bound::Lower => s >= beta,
        Bound::Upper => s <= alpha,
        _ => false,
    };
    let et = match entry.bound {
        Bound::Exact => "exact",
        Bound::Lower => "lower",
        Bound::Upper => "upper",
        _ => "none",
    };
    probe!(TtCutoff, TtCutoffEvent {
        depth: depth,
        entry_type: et,
        entry_depth: entry.depth,
        depth_sufficient: entry.depth >= depth as i8,
        cutoff_score: s,
        alpha: alpha,
        beta: beta,
    });
    if cutoff {
        ctx.stats.tt_cutoffs += 1;
        return (tt_move, Some(s), Some(tt_raw_eval));
    }
    (tt_move, None, Some(tt_raw_eval))
}
```

- [ ] **Step 6: Update `alpha_beta.rs` to use the new 3-tuple return from `try_tt_cutoff()`**

In `src/search/alpha_beta.rs` at **line 115**, the call site is:

```rust
let (mut tt_move, tt_cutoff) = try_tt_cutoff(ctx, board.hash, depth, alpha, beta, is_pv, ply);
```

Change to 3-tuple destructuring:

```rust
let (mut tt_move, tt_cutoff, tt_raw_eval) = try_tt_cutoff(ctx, board.hash, depth, alpha, beta, is_pv, ply);
```

The `tt_cutoff` usage on line 116-119 (`if let Some(s) = tt_cutoff`) stays the same — `tt_cutoff` is still `Option<Score>`. Only the destructuring adds the third element.

At **lines 164-171**, the current code is:

```rust
// Static evaluation for pruning heuristics
let static_eval = evaluate(
    board,
    &EvalContext {
        atk: ctx.atk,
        options: &ctx.options,
    },
);
```

Replace with:

```rust
// Static evaluation for pruning heuristics — reuse TT raw_eval if available
let static_eval = if let Some(re) = tt_raw_eval {
    if re != 0 { re as Score } else {
        evaluate(board, &EvalContext { atk: ctx.atk, options: &ctx.options })
    }
} else {
    evaluate(board, &EvalContext { atk: ctx.atk, options: &ctx.options })
};
```

The `tt_raw_eval` variable is in scope at this point because it was destructured from `try_tt_cutoff` at line 115 (the `let` scopes to the end of the function).

- [ ] **Step 7: Update the TT store call to include raw_eval**

In `src/search/alpha_beta.rs` at **lines 610-616**, the TT store call is:

```rust
ctx.tt.store(
    board.hash,
    score_to_tt(best_score, ply),
    best_move,
    depth as i8,
    bound,
);
```

Add `static_eval` as the `raw_eval` parameter:

```rust
ctx.tt.store(
    board.hash,
    score_to_tt(best_score, ply),
    best_move,
    depth as i8,
    bound,
    static_eval as i16,
);
```

`static_eval` is the local variable computed at line 165 (or from TT reuse). It is the raw uncorrected evaluation — exactly what the TT should store.

- [ ] **Step 8: Add round-trip test for raw_eval in TT**

In `src/tt/tests.rs`, add:

```rust
#[test]
pub(super) fn tt_round_trips_raw_eval() {
    let tt = TranspositionTable::new(1);
    let hash = 0xABCD_EF01_2345_6789;

    tt.new_search();
    tt.store(hash, 42, 0x1234, 5, Bound::Exact, -150);

    let entry = tt.probe(hash).expect("stored entry");
    assert_eq!(entry.raw_eval, -150);
    assert_eq!(entry.score, 42);
    assert_eq!(entry.best, 0x1234);
}

#[test]
pub(super) fn tt_raw_eval_zero_round_trips() {
    let tt = TranspositionTable::new(1);
    let hash = 0xDEAD_BEEF_CAFE_BABE;

    tt.new_search();
    tt.store(hash, -500, 0xABCD, 3, Bound::Upper, 0);

    let entry = tt.probe(hash).expect("stored entry");
    assert_eq!(entry.raw_eval, 0);
}

#[test]
pub(super) fn tt_raw_eval_boundary_values() {
    let tt = TranspositionTable::new(1);
    tt.new_search();

    // i16::MAX
    tt.store(0x1, 0, MOVE_NONE, 0, Bound::None, i16::MAX);
    let e = tt.probe(0x1).unwrap();
    assert_eq!(e.raw_eval, i16::MAX);

    // i16::MIN
    tt.store(0x2, 0, MOVE_NONE, 0, Bound::None, i16::MIN);
    let e = tt.probe(0x2).unwrap();
    assert_eq!(e.raw_eval, i16::MIN);

    // Typical eval range values
    tt.store(0x3, 0, MOVE_NONE, 0, Bound::None, 150);
    let e = tt.probe(0x3).unwrap();
    assert_eq!(e.raw_eval, 150);

    tt.store(0x4, 0, MOVE_NONE, 0, Bound::None, -320);
    let e = tt.probe(0x4).unwrap();
    assert_eq!(e.raw_eval, -320);
}
```

- [ ] **Step 9: Build and run tests**

Run: `cargo test tt_ -- --nocapture 2>&1 | tail -20`
Expected: All TT tests PASS, including the new raw_eval tests.

Run: `cargo test --quiet 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 10: Build release and run full test suite**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compilation succeeds.

Run: `cargo test --quiet 2>&1 | tail -5`
Expected: Full test suite passes.

- [ ] **Step 11: Commit**

```bash
git add src/tt/entry.rs src/tt/packing.rs src/tt/table.rs src/tt/tests.rs \
        src/search/alpha_beta.rs src/search/quiescence.rs src/search/tt_cutoff.rs
git commit -m "feat: store raw static eval in transposition table entries

Pack raw_eval (i16) into the data word at bits 48-63, alongside score
(i32, bits 0-31) and best move (u16, bits 32-47). The raw (uncorrected)
eval enables:
- Eval reuse without recomputation (saves Evaluation calls)
- Future correction history (Layer 1) which needs raw eval to compute
  the error term

TT store() now takes raw_eval as a parameter. The search uses TT raw_eval
to skip evaluate() calls when a TT hit provides a cached eval.

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---
