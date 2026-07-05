# Boa Engineering Architecture

**The complete specification for building a marvel of engineering in
computer chess.**  Every layer, every system, every dependency, every
decision — with the engineering oath, the verified top-engine data,
and the Boa-specific gap analysis all integrated into one executable
plan.

**Related specifications:**
- Probe system design: `docs/superpowers/specs/2026-06-29-probe-system-design.md`
  — the observability infrastructure that every subsystem in this document
  depends on.  Any subsystem that adds probe events follows the pattern:
  struct in `src/probe/events.rs` → variant in `ProbeEvent` → `probe!()`
  calls → field legend in `meta_json()`.
- Layer 6 (Evaluation) will receive its own dedicated specification
  before implementation begins.  This document's Layer 6 section is a
  roadmap, not an implementation spec.

---

## The Boa Engineering Oath

Every change to this engine must satisfy:

1. **Correctness before optimization.**  Bugs and correctness issues are
   fixed before any performance or Elo work.  No heuristic can be trusted
   on a broken foundation.

2. **Information quality before selectivity.**  Pruning and reduction
   decisions are only as good as the signals they read.  Continuation
   history, correction history, and TT quality come before pruning
   refinements.

3. **Architecture before heuristics.**  Build the structural layer that a
   heuristic depends on before implementing the heuristic.  No variance
   model to compensate for weak move ordering.  No ML to compensate for
   inaccurate evaluation.  Fix the lower layer first.

4. **Measurement before intuition.**  Every decision has a measurement
   behind it — SPRT at fast time control, paired A/B on identical
   decisions, or a calibration sweep.  No "this feels right" or
   "Stockfish does this."

5. **Research only on a trustworthy baseline.**  The baseline must
   include continuation history, correction history, clustered TT,
   verified NMP, ProbCut, and singular extensions before any novel ML
   or search research result can be considered reliable.

6. **Every new idea must beat the strongest classical implementation we
   can build — not the weakest one we happened to have.**  A +5 Elo
   result against a bare baseline does not mean the idea is good.  It
   means the baseline was weak.  Test against the best classical
   version of the same mechanism.

7. **Observability before optimization.**  Every subsystem must expose
   diagnostics before it is tuned.  Before continuation history is
   "done": probe hit rate, average score contribution, score
   distribution, table saturation, update frequency.  Before correction
   history is "done": correction histogram, average correction, RMS
   correction, correlation with search error.  Before ProbCut is
   "done": attempts, acceptance rate, false positives, node savings,
   tactical misses.  Before singular extensions are "done": trigger
   frequency, extension frequency, average depth increase, tactical
   conversion rate.  If you cannot answer "how often does this fire
   and was it right?" from probe data, the subsystem is not done.

---

## The Dependency Rule

```
Nothing above may compensate for something below.
```

This is the architectural invariant.  Every layer physically depends
on the one beneath it.  A system in Layer N may only be built when all
systems in Layers 0 through N-1 that it depends on are complete and
SPRT-validated.

The rule is violated when:
- ML is used to compensate for weak move ordering
- Variance models are used to compensate for inaccurate evaluation
- Pruning aggressiveness is tuned to compensate for TT collisions
- Time is wasted re-searching because of missing extensions

The correct response to a problem in Layer N is always: **fix the
lower layer that caused it.**

---

## The Seven-Layer Architecture

```
Layer 0 — Correctness       SEE pins, gravity history, TT raw eval storage
Layer 1 — Information        continuation history (1,2,4,6), correction history, pawn history
Layer 2 — Memory             clustered TT, TT qsearch, replacement policy
Layer 3 — Selectivity        verified NMP, ProbCut, razoring, LMR refinement
Layer 4 — Tactical Depth     singular extensions, multi-cut, threat/recapture extensions
Layer 5 — Time               PV stability, score trend, node distribution, easy move
Layer 6 — Evaluation         threat terms, passed pawn, king safety, texel tuning, NNUE
Layer 7 — Research           criticality LMR, variance pruning, ML experiments
```

Boa's current state: partial Layer 0, fragments of Layer 1, a minimal
Layer 3, and experimental work in Layer 7.  The engineering priority
is building Layers 0 through 6 in order before continuing Layer 7
research.

---

## Section 1 — The Top-Engine Evidence Base

Every claim in this document is verified against the source code of
all 8 strongest open-source engines: Stockfish, Reckless, PlentyChess,
Obsidian, Ethereal, Berserk, Marvin, and Caissa (June 2026).

### 1.1 — Universal (8/8 engines)

| System | Prevalence | What it means |
|--------|-----------|---------------|
| Continuation history | 8/8 | Multiple offset tables indexed by previous moves. Minimum 2 offsets (Ethereal, Marvin). Typical 4 offsets (Reckless, PlentyChess, Obsidian, Berserk, Caissa). Maximum 6 offsets (Stockfish). |
| Singular extensions | 8/8 | Reduced-depth verification search excluding the TT move. If no alternative comes close, extend. Multi-cut when verification shows multiple good moves. Negative extensions when the TT move is proven not singular. |
| Clustered TT | 8/8 | Set-associative buckets. 3 entries/bucket for 7 engines, 5 for PlentyChess. Multiplication-based indexing. |
| ProbCut | 8/8 | Search captures at reduced depth against `beta + margin`. Margin range: 100-214 cp. |
| LMR with history adjustment | 8/8 | `log(depth) × log(moves) / divisor`. Good history → less reduction. Check → less reduction. Killer/counter → less reduction. PV → less reduction. |
| Gravity-based history aging | 8/8 | `new = old + delta − old × |delta| / GRAVITY`. GRAVITY = 16384 (most engines) or 65536 (Stockfish). |
| Depth-scaled history bonus | 8/8 | Best move on beta cutoff gets bonus that grows with depth: `min(K1 × d² + K2 × d + K3, MAX)`. |

### 1.2 — Near-universal (6–7/8 engines)

| System | Engines with it | Engines without it |
|--------|----------------|-------------------|
| Correction history | Stockfish, Reckless, PlentyChess, Obsidian, Berserk, Caissa | Ethereal, Marvin |
| Verified NMP | Stockfish, Reckless, PlentyChess, Berserk, Caissa, Marvin | Obsidian, Ethereal |
| TT in qsearch (probe + store) | Stockfish, Reckless, PlentyChess, Obsidian, Ethereal, Berserk, Caissa | Marvin (probes only) |
| Pawn history table | Stockfish, Reckless, PlentyChess, Obsidian, Berserk | Ethereal, Marvin, Caissa |
| Multi-factor time management | All except Marvin | Marvin |
| Quiet checks in qsearch | 6/8 | Marvin, Ethereal |

### 1.3 — The engines without correction history are the weakest

Ethereal and Marvin — the only two engines without correction history —
are the lowest-rated engines in the set.  This is not coincidence.
Correction history debiases static eval before it feeds into RFP, NMP,
and futility margins.  When eval is systematically wrong for certain
position types, the correction compensates.  Every pruning decision
becomes more accurate.

### 1.4 — The universal data flow

Every top engine follows the same architecture:

```
Correction History → debiases Static Eval
Static Eval → feeds RFP, NMP, FFP, razoring margins
Continuation History → sharpens Move Ordering
Better Move Ordering → more first-move cutoffs → LMR can be more aggressive
LMR → uses History scores for reduction adjustment
Singular Extensions → catch what LMR would miss
ProbCut → uses qsearch+SEE for fast pre-rejection of losing captures
TT → caches everything, enables transpositions
TT in qsearch → saves nodes across the majority of the tree
```

Every missing piece degrades every other piece.  When Boa lacks
continuation history, move ordering is weaker → fewer first-move
cutoffs → LMR must be more conservative → less depth → weaker play.
The variance-aware pruning is trying to compensate for positional
uncertainty that sharper history would resolve directly.

---

## Section 2 — Layer 0: Correctness

**Purpose:** Remove all known bugs and correctness issues from the
foundation.  No heuristic can be trusted on a broken foundation.

**Dependencies:** None.  These are the bottom layer.

**Rule:** No experiments.  No SPRT required to justify — these are
correctness fixes.  SPRT is only needed to confirm nothing was broken.

### 2.1 — SEE pin exclusion

**Status:** Missing.  `src/search/see.rs` computes `attackers_to()` and
`least_valuable_attacker()` using standard attack tables but never
checks whether the attacker is absolutely pinned to the king.

**What it is:** During static exchange evaluation, an attacker that is
absolutely pinned to its own king cannot legally capture on the target
square.  Failing to exclude pinned attackers causes SEE to overestimate
the gain from captures, leading to incorrect pruning decisions where
the engine thinks a capture wins material but the piece is actually
pinned and cannot recapture.

**What to build:**
- In `least_valuable_attacker()`, after finding a candidate attacker,
  check whether it is absolutely pinned.  An attacker is absolutely
  pinned if it lies on a ray between its king and an enemy sliding
  piece, with no other pieces blocking.
- Add a helper: `fn is_pinned(board, square, color) -> bool` that
  checks for absolute pins along rook/bishop rays.
- After finding the least valuable attacker, if it is pinned, skip it
  and look for the next least valuable attacker.

**Top engine standard:** Every top engine handles pins in SEE.  Not
handling them is a correctness bug, not a tuning question.

**Verification:** Run SEE on known pin positions and compare with a
reference (Stockfish's SEE).  Perft validation should not regress.

**Observability:** Add a probe event `Se` (already exists for SEE
decisions in qsearch).  Extend it to record whether any legal attacker
was excluded due to a pin, so the frequency of pin-affected SEE
decisions is measurable.  This is a correctness fix, not a heuristic —
the probe confirms it fires in real positions.

### 2.2 — Gravity-based history updates

**Status:** Boa uses overflow-triggered division-by-2 aging.
`src/search/move_ordering.rs:139-150` and `:153-160`.

**What it is:** Replace the current `HISTORY_OVERFLOW_THRESHOLD` +
`scale_down_history()` mechanism with the standard gravity formula:

```
new_value = old_value + delta - old_value * abs(delta) / GRAVITY
```

Where GRAVITY is 16384 (2¹⁴).  This formula:
- Naturally prevents overflow — values asymptotically approach ±GRAVITY
- Provides continuous, proportional decay — no abrupt division-by-2
- Is self-stabilizing — no overflow threshold to tune
- Is universal across all 8 top engines

**What to change:**
1. In `add_history_score()`: replace `if ctx.history[ci][pt][to].abs() > HISTORY_OVERFLOW_THRESHOLD { scale_down_history(ctx, ci); }` with the gravity formula.
2. Apply the same change to `scale_down_cap_history()` and `update_cap_history()`.
3. Keep `history_delta(depth)` = `depth * depth` for now (the bonus formula improves in Layer 1).
4. Initialize all history tables to small negative values (e.g., -5 for quiet history, -700 for capture history), not zero.  This biases against unproven moves.

**Verification:** `cargo test`.  No functional change — the search tree should be identical modulo floating-point differences in history values.  `cargo bench` or a fixed-depth search should produce identical node counts.

**History bonus formula:** The `history_delta(depth) = depth²` formula is
replaced in Layer 1 (section 3.0) with a depth-scaled linear+cap formula
from the top engines.  The gravity formula implemented here is the
*update mechanism*; the bonus formula is the *update magnitude*.  Both
must change together — using the gravity mechanism with the old `depth²`
bonus is acceptable as a transitional state but the upgrade happens in
Layer 1.

### 2.3 — TT raw eval storage

**Status:** Missing.  `src/tt/entry.rs` has `TtEntry { key, score, best, depth, bound, age }` — no static eval field.

**What it is:** Store the raw (uncorrected) static evaluation in each TT entry.  This enables:
- Eval reuse without recomputation (saves Evaluation calls)
- Correction history (Layer 1) — correction history needs the raw eval to compute the error term
- Future correction-aware pruning

**Entry layout (64-bit packed):**

The current `AtomicTtSlot` packs into a single 64-bit word.  The new layout:

```
┌───────────────── 64 bits ─────────────────┐
│ bits 63-48 │ bits 47-32 │ bits 31-0       │
│ raw_eval   │ score      │ ctrl/data       │
│ i16        │ i16        │ see below       │
└───────────────── 64 bits ─────────────────┘

ctrl/data (bits 31-0):
  bits 31-16: key16 (top 16 bits of full hash, for collision detection)
  bits 15-8:  depth (i8, offset by DEPTH_NONE = -3)
  bits 7-2:   age (u6, 0-63, incremented each search)
  bits 1-0:   bound (2 bits: 00=none, 01=exact, 10=lower, 11=upper)

Best move: stored in a separate AtomicU16 alongside the slot
  (move is 16 bits — from 6 bits, to 6 bits, flags 4 bits)
```

This packs everything into 64 bits + 16 bits = 80 bits total per slot.
Two `AtomicU64` fields or one `AtomicU64` + one `AtomicU16` in the slot.

The key design decision: `raw_eval` is stored *uncorrected*.  When
correction history is added (Layer 1, section 3.5), the correction is
computed from the raw eval at probe time, not at store time.  This means
a single TT entry can serve searches with different correction states.

**What to change:**
1. Add `raw_eval: i16` to the logical `TtEntry` struct.
2. In `pack_entry()` / `unpack_entry()` in `src/tt/packing.rs`, add the
   raw_eval field at bits 63-48.
3. Update `AtomicTtSlot` to hold two atomics: `ctrl: AtomicU64` (packed
   raw_eval + score + key16 + depth + age + bound) and `best: AtomicU16`
   (best move).  Or pack raw_eval into the existing 64-bit word as shown
   above and keep best move in a separate atomic.
4. In `tt/store()`, accept and pack `raw_eval`.
5. In `tt/probe()`, unpack and expose `raw_eval` in the returned `TtEntry`.
6. In `alpha_beta.rs` and `quiescence.rs`, when a TT hit provides
   `raw_eval`, use it instead of calling `evaluate()`.  The saved eval
   call is the primary benefit until correction history is added.

**Verification:** `cargo test`.  Elo-neutral by design — this is
infrastructure.  `bench` should show a small speedup from avoided eval
recomputation.  Verify that the packed entry round-trips correctly:
`unpack(pack(entry)) == entry` for a range of test values.

### 2.4 — Non-zero history initialization

**Status:** Boa initializes all history tables to zero.
`src/search/context.rs:99-101`.

**What it is:** Every top engine initializes history tables to small
negative values (typically -5 to -700).  This biases against unproven
moves — a move with zero history (never tried) should score below a
move with slightly negative history (tried once and failed), which
should score below a move with positive history (tried and succeeded).

**What to change:**
- Butterfly history: initialize to -5
- Capture history: initialize to -700
- Continuation history (when added in Layer 1): initialize to -552

**Verification:** `cargo test`.  Minor ordering change; run SPRT to
confirm no regression.

---

## Section 3 — Layer 1: Information

**Purpose:** Build the signal infrastructure that every selectivity
and pruning decision depends on.  This is the most important layer.

**Dependencies:** Layer 0 complete (gravity history, TT raw eval).

**Rule:** Build 1-ply continuation history first, validate with SPRT,
then 2-ply, validate, then 4/6.  Do not build all tables at once.
Each increment must prove itself.

**Retention rule:** The existing killer moves and counter-move heuristic
are retained during Layer 1.  Do not remove or reduce their weight because
Reckless did.  Only revisit their weighting after continuation history is
complete and independently SPRT-tested.  If measurement shows they are
redundant, remove them then — not before.

### 3.0 — History bonus formula upgrade

The bonus formula determines how much the best quiet move's history score
increases on a beta cutoff.  Boa currently uses `bonus = depth * depth`.
Every top engine uses a linear+cap formula.  The bonus formula directly
affects what the continuation history tables learn — upgrading it is
part of the information layer, not a later refinement.

**What to change:**

Replace `history_delta(depth)` in `src/search/move_ordering.rs`:

```
// Old:
fn history_delta(depth: i32) -> i32 {
    depth * depth
}

// New:
fn history_delta(depth: i32, is_strong_cutoff: bool) -> i32 {
    let d = depth + if is_strong_cutoff { 1 } else { 0 };
    // Obsidian-style formula: linear growth, capped at 1409
    (175 * d + 15).min(1409)
}
```

The `is_strong_cutoff` flag is true when `best_score > beta + 75`
(the cutoff was well above beta, indicating a genuinely strong move).
This is standard in Stockfish, Ethereal, Berserk, and Obsidian.

**Malus for failed quiet moves:**

Quiet moves that were searched but did not cause a beta cutoff receive
a negative update (malus).  The malus formula:

```
fn history_malus(depth: i32) -> i32 {
    -(196 * depth - 25).min(1047).max(-1047)
    // Obsidian-style: slightly larger malus than bonus for asymmetry
}
```

Apply this malus to the butterfly history and continuation history for
each quiet move that was searched but failed to beat alpha.

**Verification:** `cargo test`.  SPRT at fast time control — expected
+3 to +8 Elo from the bonus formula upgrade alone (before continuation
history is added).  If Elo-neutral, the formula constants may need
tuning, but the architecture (linear+cap) is correct.

### 3.1 — Continuation history (1-ply)

**Status:** Missing.  Boa has butterfly history `[color][piece][to]`
but nothing indexed by the previous move.

**What it is:** A history table indexed by both the *current* move and
the *previous* move.  Instead of one global table `history[piece][to]`,
you have `cont1[prev_piece][prev_to][piece][to]`.  This captures
context: a knight-to-f3 has a very different value after e2-e4 than
after h7-h5.

**Every top engine has this.**  The minimum is 2 offsets (Ethereal's
counter-move + follow-up, Marvin's counter + follow).  The typical is
4 offsets (Reckless, PlentyChess, Obsidian, Berserk use 1, 2, 4, 6).
Stockfish uses 6 offsets.

**What to build (1-ply only first):**

1. **Table:** `cont1: [[[i32; 64]; 64]; 6]` — `[prev_piece][prev_to][piece][to]`.
   Dimensions: 6 × 64 × 6 × 64 = 147,456 entries × 4 bytes = ~576 KB.
   Initialize to -552.

2. **Pointer setup:** In `alpha_beta()`, after making a quiet move, set
   `ctx.stack[ply].cont_hist = &cont1[moved_piece][move_to]` — this is
   the pointer the *child* node will read.  The child reads
   `(*(ss-1)->cont_hist)[child_piece][child_to]` to get the continuation
   history score.  This is how all top engines do it — a pointer set by
   the parent, dereferenced by the child.

3. **Scoring:** In `score_single_move()`, add the continuation history
   score to quiet moves:
   ```
   if ply > 0 && ply < 128 {
       let prev_cont = ctx.stack[ply - 1].cont_hist;
       if let Some(cont) = prev_cont {
           s += cont[piece_type(mover) as usize][move_to(m) as usize];
       }
   }
   ```

4. **Update:** In `handle_beta_cutoff()`, for the best quiet move, apply
   a depth-scaled bonus from section 3.0:
   ```
   let bonus = history_delta(depth, best_score > beta + 75);
   cont1[prev_piece][prev_to][piece][to] += bonus
       - cont1[prev_piece][prev_to][piece][to] * bonus.abs() / 16384;
   ```
   For quiets that failed to cause a cutoff, apply `history_malus(depth)`
   as a negative update.

5. **50-move and repetition safety:** Do NOT update continuation
   history when the same position has appeared before (repetition) or
   when the 50-move counter is high.  These positions are not
   representative of normal play.

**SPRT validation:** Run at fast time control (1+0.01s or similar).
Expected: +15 to +30 Elo.  If no gain, something is wrong in the
implementation — continuation history at even 1 ply is universally
positive.

**Observability (1-ply):** Before SPRT, add probe events for:
- `ch_hit_rate`: what fraction of quiet moves get a non-zero continuation
  history score (should be > 50% after a few plies of search)
- `ch_avg_score`: the average contribution of cont history to quiet move
  scores (should be significantly non-zero, indicating the table is
  learning)
- `ch_saturation`: fraction of entries near ±GRAVITY (if > 20%, the
  table is too small or the gravity constant is wrong)
- `ch_update_freq`: updates per search (should scale with nodes searched)
- `ch_score_distribution`: histogram of score values (should be roughly
  symmetric around a small negative mean — most moves are average, a few
  are very good or very bad)

These probes confirm the table is actually learning before SPRT is run.
If `ch_avg_score` is near zero after 10K nodes, the update logic is
broken.  If `ch_saturation` is > 50%, the gravity constant is wrong.

### 3.2 — Continuation history (2-ply)

**After 1-ply SPRT passes:**

**Table:** `cont2: [[[i32; 64]; 64]; 6]` — same dimensions, same
initialization.  This is indexed by the move two plies ago.

**Scoring:** Add `cont2[prev2_piece][prev2_to][piece][to]` to the
quiet move score, with a weight of ~0.7× relative to the 1-ply weight
(since the correlation is weaker at longer distance).

**Update:** Same gravity formula, but apply half the bonus at 2-ply
distance (since the causal link is weaker).

**SPRT validation:** Expected additional +5 to +15 Elo.

### 3.3 — Continuation history (4,6-ply)

**After 2-ply SPRT passes:**

Same pattern for offsets 4 and 6.  Use quarter bonus for these
distances.  The scoring weights for offset 4 and 6 should be lower
than 1-ply and 2-ply (the standard range is 0.3-0.5× relative to
offset 1).

**Offsets 3 and 5:** Most engines skip these (Reckless, PlentyChess,
Obsidian, Berserk all skip offsets 3 and 5).  Stockfish uses them
but this is a tunable detail discovered after all other infrastructure
was solid.  Start without them.  Add later only if SPRT justifies.

**SPRT validation:** Expected additional +5 to +10 Elo total for
offsets 4 and 6 combined.

### 3.4 — Pawn history

**Status:** Missing.  5 of 8 top engines have it.

**What it is:** A history table keyed by pawn structure hash instead of
the previous move.  `pawn_hist[pawn_key % 1024][piece][to]` provides
position-type-aware history that captures patterns like "knight-to-f3
is strong in this pawn structure regardless of what the previous move
was."

**What to build:**
- Table: `pawn_history: [[[i32; 64]; 6]; 1024]` — 1024 × 6 × 64 = 393,216 entries.
  Keyed by `board.pawn_hash() & 1023`.
- Add to quiet move scoring with weight ~0.5× relative to butterfly.
- Update on beta cutoffs with the same gravity formula.

**SPRT validation:** Expected +5 to +10 Elo, but this is more
position-dependent than continuation history.  The gain comes from
better ordering in closed positions and endgames where pawn structure
dominates tactics.

### 3.5 — Correction history

**Status:** Missing.  6 of 8 top engines have it.

**What it is:** An online statistical correction to static eval.  The
engine learns from its search results that certain position types are
systematically over- or under-evaluated, and corrects the eval before
it feeds into pruning margins.

**Correction history data flow:**

Reading (before movegen, feeds into pruning margins):

```
TT probe
    │
    ▼
raw_eval (from TT hit, or fresh evaluate())
    │
    ▼
correction_history(raw_eval, board, stack)
    │
    ▼
corrected_eval = raw_eval + correction / DIVISOR
    │
    ├──► RFP margin computation
    ├──► NMP margin computation
    ├──► Razoring margin computation
    ├──► ProbCut margin computation
    ├──► FFP margin computation
    └──► LMR reduction adjustment
```

Writing (after search returns from a node):

```
search returns best_score
    │
    ▼
diff = best_score - raw_eval
    │
    ▼
bonus = clamp(diff * depth / 4, -CORRHIST_LIMIT / 4, +CORRHIST_LIMIT / 4)
    │
    ├──► pawn_corr[stm][pawn_hash] += gravity_update(bonus)
    ├──► nonpawn_corr_w[stm][non_pawn_hash_w] += gravity_update(bonus)
    ├──► nonpawn_corr_b[stm][non_pawn_hash_b] += gravity_update(bonus)
    ├──► cont_corr[stm][prev_piece_to][prev2_piece_to] += gravity_update(bonus)  [if ply >= 2]
    └──► cont_corr[stm][prev_piece_to][prev4_piece_to] += gravity_update(bonus)  [if ply >= 4]
    │
    ▼
TT.store(raw_eval)  // raw_eval, not corrected_eval — see section 2.3
```

**Critical placement rules:**

1. Correction is applied ONCE per node, before any pruning decision.
   The corrected eval is used for RFP, NMP, razoring, ProbCut, FFP, and
   LMR.  The raw eval is what gets stored in the TT (section 2.3) and
   what gets written to the stack (`ctx.stack[ply].static_eval`).

2. The correction tables are updated ONCE per node, after the search
   returns `best_score`.  The update uses `best_score - raw_eval` (not
   `best_score - corrected_eval`).  This is critical: the correction
   learns the *total* eval error, not the residual after correction.

3. The correction tables are per-thread (not shared).  Each thread
   learns independently from its own search results.  This is the
   standard design in all 6 engines that have correction history.

**Architecture (based on Reckless/Obsidian pattern):**

Four tables:

1. **Pawn correction:** `pawn_corr[2][16384]` — keyed by
   `[side_to_move][pawn_hash % 16384]`.  Tracks eval error for
   specific pawn structures.

2. **Non-pawn correction (White):** `nonpawn_corr_w[2][16384]` —
   keyed by `[side_to_move][non_pawn_hash(White) % 16384]`.  Tracks
   eval error for White's piece configuration.

3. **Non-pawn correction (Black):** `nonpawn_corr_b[2][16384]` —
   same for Black.

4. **Continuation correction:** `cont_corr[2][384][384]` —
   keyed by `[side_to_move][prev_piece_to][prev2_piece_to]`.
   Tracks eval error conditioned on the previous moves.

**Correction computation:**
```
corr = w1 × pawn_corr[stm][pawn_hash % 16384]
     + w2 × nonpawn_corr_w[stm][non_pawn_hash_w % 16384]
     + w2 × nonpawn_corr_b[stm][non_pawn_hash_b % 16384]

if ply >= 2:
    corr += w3 × cont_corr[stm][prev_piece_to][prev2_piece_to]

if ply >= 4:
    corr += w3 × cont_corr[stm][prev_piece_to][prev4_piece_to]

return corr / 512
```

Weights from the top engines: w1 ≈ 30–53, w2 ≈ 35–65, w3 ≈ 27–76.
Start with: w1 = 30, w2 = 35, w3 = 27.  Tune via SPRT or texel later.

**Application:**
```
corrected_eval = raw_eval + correction_value
```
This corrected eval feeds into RFP, NMP, FFP, and futility margins.
Store `raw_eval` in the TT (Layer 0 item 2.3) — the correction is
computed from the raw eval at probe time.

**Update:**
After search returns from a node:
```
diff = best_score - raw_eval
bonus = clamp(diff * depth / 4, -CORRHIST_LIMIT / 4, +CORRHIST_LIMIT / 4)
// Where CORRHIST_LIMIT = 1024

pawn_corr[stm][pawn_hash] += bonus - pawn_corr[...] * abs(bonus) / 1024
// Same gravity update for all tables
```

**Critical detail:** The correction tables are per-thread (not shared).
Each thread learns independently from its own search results.  History
tables are the same — per-thread.

**SPRT validation:** Expected +8 to +20 Elo.  The gain comes from more
accurate pruning decisions across the board.  The engines without
correction history (Ethereal, Marvin) are the weakest in the set for
a reason.

**Observability:** Before SPRT, add probe events for:
- `corr_histogram`: distribution of correction values (should be centered
  near zero with a standard deviation of 10-30 cp — if the mean is far
  from zero, the eval has a systematic bias that correction is fixing)
- `corr_avg`: running mean of correction values per search
- `corr_rms`: root mean square of correction values (measures how much
  the eval is being corrected — should be 5-30 cp for most positions)
- `corr_search_error_correlation`: correlation between correction
  magnitude and `(best_score - raw_eval)` (should be > 0.5 — the
  correction should predict actual search error)
- `corr_update_freq`: updates per search per table
- `corr_saturation`: fraction of entries near ±CORRHIST_LIMIT

The `corr_search_error_correlation` is the key diagnostic.  If the
correction history is not correlated with actual search error, it is
adding noise, not signal.  Kill it and debug before SPRT.

---

## Section 4 — Layer 2: Memory

**Purpose:** The TT becomes the engine's memory — a low-collision,
persistent cache that serves both the main search and quiescence, and
stores everything needed for correction history to work.

**Dependencies:** Layer 0 (raw eval in TT), Layer 1 (correction history
uses raw eval from TT).

### 4.1 — Clustered TT

**Status:** Direct-mapped.  One slot per hash index.
`src/tt/table.rs:30-31`.

**What it is:** Every top engine uses 3-entry buckets.  Boa's
direct-mapped TT has 2–3× the collision rate at the same table size.
A clustered TT reduces collisions by storing multiple entries per
index and replacing the least valuable one.

**What to build:**

1. **Bucket structure:** Replace the single `AtomicTtSlot` per index
   with a `Bucket` containing 3 entries:
   ```rust
   struct Bucket {
       entries: [AtomicTtSlot; 3],
       _pad: u16,  // 3 × 10 bytes + 2 pad = 32 bytes = one cache line
   }
   ```

2. **Probe:** Scan all 3 entries in the bucket.  Match on the partial
   key (top 16-32 bits of the hash).  Return the matching entry, or
   select a replacement candidate.

3. **Replace policy:** For each candidate slot, compute a quality score:
   ```
   quality = entry_depth - 4 * age_distance
   ```
   Where `age_distance` is how many searches old the entry is (0 =
   current search, higher = older).  Replace the entry with the
   LOWEST quality.

   This means: deep entries from the current search are strongly
   protected.  Shallow entries from old searches are eagerly evicted.

4. **Store logic:** Never overwrite a matching entry from the same
   search if the stored depth is greater than the new depth.  Always
   preserve the best move if the new entry has none.

5. **Hash indexing:** Switch to multiplication-based indexing:
   ```rust
   fn index(&self, hash: u64) -> usize {
       ((hash as u128 * self.num_buckets as u128) >> 64) as usize
   }
   ```
   This avoids the power-of-two size constraint and provides better
   hash distribution.

**Verification:** `cargo test`.  `hashfull` should drop significantly
at the same table size.  Bench should show speedup from fewer
collisions.

### 4.2 — TT in quiescence search

**Status:** Missing.  `src/search/quiescence.rs` has no TT calls.

**What it is:** Probe the TT at the start of qsearch, store at the end.
Qsearch is often the majority of total nodes — no TT there means no
transposition benefit across most of the search tree.

**What to build:**

1. **Probe at qsearch entry:** Before computing stand-pat, probe the TT
   for the current position hash.  If found:
   - Use the stored static eval instead of recomputing (saves an
     Evaluation call)
   - If the stored bound is sufficient (LOWER and value ≥ beta, or
     UPPER and value ≤ alpha), return immediately — this is a TT
     cutoff in qsearch
   - Use the stored best move as the first move to try

2. **Store at qsearch exit:** After the qsearch completes, store the
   result with depth 0 and the appropriate bound (LOWER if fail-high,
   UPPER if within window).  Also store the static eval for reuse.

3. **Depth and bound:** Qsearch entries use depth 0.  They are only
   useful for other qsearch nodes (not for main search nodes, which
   need depth > 0).  Don't let qsearch entries pollute the main search
   — the depth check in the main search TT cutoff naturally handles
   this.

**Verification:** `cargo test`.  Expected node count reduction of
10–30% in qsearch (measurable in bench with qsearch node counters).

### 4.3 — Replacement policy refinement

**After clustered TT and qsearch TT pass SPRT:**

Fine-tune the replacement policy:
- Adjust the age penalty weight (currently 4)
- Consider protecting exact-bound entries more strongly
- Consider always-replace for depth-0 (qsearch) entries that conflict
  with higher-depth entries

These are small Elo items — +2 to +5 each, verified via SPRT.

---

## Section 5 — Layer 3: Selectivity

**Purpose:** Pruning becomes trustworthy.  With accurate eval
(correction history) and sharp move ordering (continuation history),
the engine can prune more aggressively and more accurately.

**Dependencies:** Layer 1 (continuation history for sharp move ordering,
correction history for accurate eval), Layer 2 (clustered TT for fewer
collisions, TT in qsearch for node savings).

### 5.0 — Pruning order (mandatory)

When multiple pruning methods coexist, the order they fire in is part
of the algorithm.  Earlier prunings see the uncorrected position;
later prunings see only moves that survived earlier filters.  The
following order is frozen by this specification:

```
Entry into alpha_beta()
    │
    ▼
TT cutoff check          [Layer 2, existing]
    │
    ▼
Static eval + correction [Layer 1, section 3.5]
    │
    ▼
RFP (reverse futility)   [Layer 3, existing — variance-aware]
    │  skip if: in check, PV node, depth > RFP_MAX_DEPTH
    │
    ▼
Razoring                 [Layer 3, section 5.3]
    │  skip if: PV node, in check, depth > 4
    │
    ▼
NMP (null-move pruning)  [Layer 3, section 5.1]
    │  skip if: PV node, in check, no non-pawn material, depth < 3
    │
    ▼
ProbCut                  [Layer 3, section 5.2]
    │  skip if: PV node, in check, decisive beta, TT already above probBeta
    │
    ▼
Move generation + ordering
    │
    ▼
Move loop:
    │
    ├── FFP (forward futility)   [Layer 3, existing — variance-aware]
    │      skip if: not quiet, depth > FFP_MAX_DEPTH, is TT move, is killer,
    │               is counter, in check, PV node
    │
    ├── History pruning          [Layer 3, section 5.5]
    │      skip if: not quiet, lmrDepth >= 4, history > -5000 * depth
    │
    ├── SEE pruning              [Layer 3, existing]
    │
    ├── LMR                      [Layer 3, section 5.4]
    │
    └── Full-depth search
    │
    ▼
TT store
```

Each pruning step receives the position after all previous steps.
RFP fires before NMP because if the static eval is already far above
beta, there is no point in trying the null move.  Razoring fires
before NMP because it's cheaper (just a qsearch).  NMP fires before
ProbCut because it's cheaper (null move vs generating and searching
captures).  In the move loop, FFP fires before history pruning because
the futility margin is computed from the position, not the history.

Implementation note: each pruning step must check its own guard
conditions.  The skip conditions listed above are the mandatory
minimums — additional guards (mate scores, tablebase wins, excluded
moves during singular search) are specified in each section.

### 5.1 — Verified null-move pruning

**Status:** Boa has unverified NMP with `R = 3 + depth/4`.
`src/search/null_move.rs`.

**What it is:** Two changes to the existing NMP:

**A. Formula upgrade.**  With correction history (Layer 1) providing
accurate eval, the NMP reduction formula is upgraded to include an
eval-above-beta term, matching the universal top-engine pattern:

```
R = 4 + depth / 3 + min(3, (eval - beta) / 200) + (prev_move_was_tactical as i32)
```

- Base: 4 (was 3)
- Depth term: `depth / 3` (was `depth / 4`) — slightly more aggressive
- Eval-above-beta term: `min(3, (eval - beta) / 200)` — when eval is
  far above beta, the null move is more likely to hold, so reduce more
- Tactical bonus: +1 if the previous move was a capture or promotion
  (from Ethereal/Reckless — tactical positions are less zugzwang-prone)

This formula is present in some form in 6 of 8 top engines.  The
specific constants (4, 3, 200) are starting points for SPRT tuning.

**B. Verification search.**  At high depth (≥ 14), after NMP returns
a cutoff, run a second verification search.  This confirms the cutoff
is genuine and not a zugzwang false positive.

Zugzwang occurs when having the move is a disadvantage.  NMP assumes
the opposite — that passing the move is always a disadvantage.  In
positions with few pieces, this assumption breaks.  Verified NMP
catches these cases.

**6 of 8 top engines have verified NMP.**  Stockfish, Reckless,
PlentyChess, Berserk, Caissa, and Marvin verify.  Only Obsidian
and Ethereal do not.

**What to build:**

1. Upgrade the reduction formula as above.  SPRT this change separately
   from the verification search.

2. After the null-move search returns a score ≥ beta, if `depth >= 14`,
   run a verification search at `depth - R - 4` with bounds
   `[beta - 1, beta]`.  If the verification also returns ≥ beta, the
   cutoff is accepted.  If not, the null-move cutoff is rejected and
   the search continues normally.

3. Guard against recursive null-move chains: set a flag
   `nmp_in_progress` that prevents the verification search itself from
   doing NMP.

4. The zugzwang detection can also use side-to-move material
   information: if the side to move has only pawns and a king, NMP
   should be disabled entirely (no need for verification — just skip
   NMP in these positions).

**SPRT validation:** Test the formula upgrade and verification search
separately.  Combined expected gain: +8 to +18 Elo.

### 5.2 — ProbCut

**Status:** Missing.  8/8 top engines have it.

**What it is:** Use a shallow search to predict whether a capture will
cause a beta cutoff at full depth.  If the shallow search already
scores above `beta + margin`, the engine returns early without
searching at full depth.

This is an "early cutoff" mechanism specifically for captures.  It
depends on SEE (which Boa has) and a working qsearch (which Boa has).

**What to build:**

1. **ProbCut beta:** `prob_beta = beta + 150` (start with 150, tune via SPRT).

2. **Candidate selection:** In the move loop, before the first move is
   searched, generate all captures.  For each capture with
   `SEE(move) >= prob_beta - static_eval`, do:

3. **Reduced search:** Search at `depth - 4` with a null window
   `[-prob_beta, -prob_beta + 1]`.

4. **Cutoff:** If the reduced search score ≥ prob_beta, the full-depth
   search would likely also fail high.  Return `prob_beta` immediately
   (or the reduced search score if it's higher).

5. **Skip conditions:** Don't apply ProbCut if beta is a mate score
   (decisive scores should be verified at full depth).  Don't apply if
   the TT already indicates the position is below prob_beta at
   sufficient depth.

**SPRT validation:** Expected +10 to +20 Elo.  This is one of the
largest gains in the selectivity layer because it avoids searching
captures that are obviously winning to full depth.

**Observability:** Before SPRT, add probe events for:
- `pc_attempts`: how many captures are considered for ProbCut per search
- `pc_accepted`: how many ProbCut searches return a cutoff (acceptance
  rate — should be 5-30% of attempts for the mechanism to be useful)
- `pc_false_positives`: number of times ProbCut returned a cutoff but a
  full-depth verification search (run occasionally for measurement)
  showed the score was actually below beta (must be near zero —
  ProbCut false positives cause tactical blunders)
- `pc_node_savings`: estimated nodes saved by ProbCut cutoffs (compare
  nodes spent on ProbCut searches vs nodes a full-depth search would
  have spent)
- `pc_tactical_misses`: positions where ProbCut skipped a capture that
  would have been the best move (detected via occasional full-depth
  verification of ProbCut-rejected moves)

If `pc_false_positives` > 0.1%, the margin is too tight.  If
`pc_accepted` < 2%, the margin is too wide or the reduced search depth
is insufficient.  Both must be measured before SPRT can tell you
whether the mechanism is working correctly rather than just
accidentally gaining Elo.

### 5.3 — Razoring

**Status:** Missing.  8/8 top engines have it.

**What it is:** At very shallow depth (1–4), if the static eval is far
below alpha even after adding a margin, skip the full search and return
a qsearch result directly.

**What to build:**

```
if !is_pv && !in_check && depth <= 4 && eval + 150 * depth <= alpha {
    let qscore = quiescence(board, ctx, alpha, beta, ply);
    if qscore <= alpha {
        return qscore;
    }
}
```

The margin grows with depth.  At depth 1: if eval + 150 ≤ alpha.
At depth 4: if eval + 600 ≤ alpha.

**SPRT validation:** Expected +3 to +8 Elo.  Small but nearly free —
the logic is trivial and the node savings add up.

### 5.4 — LMR refinement

**After continuation history and correction history pass SPRT:**

The LMR formula can now be refined with sharper signals:

1. **Continuation-history-based reduction:** The sum of continuation
   history scores directly adjusts the reduction amount.  Good
   continuation history → less reduction.  Bad continuation history →
   more reduction.  This replaces the simpler butterfly-only adjustment.

2. **Correction-aware reduction:** When the correction history value
   is large (the static eval is being significantly adjusted), reduce
   LMR aggressiveness.  The position is harder to evaluate than it
   looks, so prune less.

3. **Deeper/shallower re-search:** After the reduced search fails high,
   adjust the re-search depth based on how much the score improved:
   ```
   if score > best_score + 50 { newDepth += 1 }
   if score < best_score + 10 { newDepth -= 1 }
   ```
   This is standard in 7 of 8 top engines.

4. **Adjustment terms:** Add the standard adjustment terms found in
   every top engine:
   - PV nodes: less reduction
   - Cut nodes: more reduction
   - Killers/counter-moves: less reduction
   - Check-giving moves: less reduction
   - History score: less reduction for good history
   - Improving flag: less reduction when improving

**SPRT validation:** Each term should be tested individually.  Expected
cumulative gain: +15 to +30 Elo from all LMR refinements combined.

### 5.5 — History-aware pruning

**After continuation history passes SPRT:**

With continuation history providing sharp signals, add
history-dependent pruning:

1. **History pruning:** If a quiet move's total history score
   (butterfly + continuation offsets) is below a negative threshold,
   skip it.  The threshold scales with depth: `threshold = -5000 * depth`.
   Moves with deeply negative history at shallow remaining depth are
   almost certainly not going to beat alpha.

2. **Continuation pruning:** At very shallow LMR depth (1–3), if both
   the 1-ply and 2-ply continuation history are strongly negative,
   skip the move entirely.  The engine has never seen this move work
   in this context — it's unlikely to work now.

**SPRT validation:** Each pruning rule must pass independently.
Expected gain: +5 to +15 Elo combined.

---

## Section 6 — Layer 4: Tactical Depth

**Purpose:** Extend the search where it matters.  Singular extensions
catch what LMR would miss — forced lines, tactical shots, critical
recaptures.

**Dependencies:** Layer 1 (continuation history for sharp TT move
detection), Layer 2 (clustered TT for reliable TT entries), Layer 3
(verified NMP, ProbCut, refined LMR — the pruning infrastructure
that extensions complement).

### 6.1 — Singular extensions (single-level)

**Status:** Missing.  8/8 top engines have singular extensions.
Boa has only check extension.

**What it is:** When the TT move is significantly better than all
alternatives, the position likely contains a forced tactical sequence.
Extend the TT move by one ply to see deeper into the forcing line.

**What to build (single-level only first):**

1. **Trigger condition:**
   ```
   depth >= 8
   && move == tt_move
   && tt_entry.depth >= depth - 3
   && tt_entry.bound is LOWER or EXACT
   && abs(tt_entry.score) < MATE_SCORE
   && !is_root
   ```

2. **Singular beta:**
   ```
   singular_beta = tt_score - 2 * depth
   ```
   The margin grows with depth because the TT score is more reliable
   at higher depths.

3. **Singular search:** Run a reduced-depth search at `(depth - 1) / 2`,
   excluding the TT move, with a null window `[singular_beta - 1, singular_beta]`.

4. **If singular** (verification score < singular_beta): extend by +1 ply.
   No other move came close to the TT move at reduced depth — it is
   likely a forced line.

5. **If not singular** (verification score >= singular_beta):
   - If `singular_beta >= beta`: **multi-cut** — multiple moves beat
     beta at reduced depth.  Return beta immediately.
   - If `tt_score >= beta`: the TT move is not singular but still good
     at full depth.  No extension, no penalty.
   - If `tt_score <= alpha`: the TT entry was misleading (failing low).
     No extension.

6. **Safety caps:**
   - Don't extend more than one ply in this first version
   - Cap total extension per line (max 6–8 cumulative extensions)
   - Don't extend if already in a singular search (prevent recursion)
   - Don't extend if the search is in a check-evasion node

**SPRT validation:** Expected +15 to +30 Elo.  This is one of the
largest single Elo sources among features Boa is missing.  Every top
engine has it.

**Observability:** Before SPRT, add probe events for:
- `se_trigger_freq`: how often the TT move meets the singular extension
  trigger conditions (depth ≥ 8, ttMove present, ttDepth ≥ depth-3,
  ttBound is LOWER).  Should be 5-20% of nodes above depth 8.
- `se_extension_freq`: how often the verification search confirms
  singularity and an extension is actually applied.  Should be 2-10%
  of triggers — most TT moves are good but not singular.
- `se_avg_depth_increase`: the average extension amount (should be
  close to 1.0 for single-level SE — multi-level adds later)
- `se_tactical_conversion`: in positions where a singular extension
  fired, does the engine find a tactical sequence (score change > 50 cp
  within the extended depth) that it would have missed?  Measure by
  comparing the score at the extended depth vs the score at the
  unextended depth.
- `se_multi_cut_freq`: how often multi-cut fires (verification shows
  multiple moves beat singularBeta).  Should be 1-5% of triggers.
- `se_negative_ext_freq`: how often a negative extension is applied
  (TT move is proven not singular).  Should be 5-15% of triggers.

The `se_tactical_conversion` metric is the key one.  If singular
extensions fire but don't change the score, they're wasting nodes.
If they fire and consistently find tactical shots, they're working.

### 6.2 — Multi-cut

**After single-level SE passes SPRT:**

Multi-cut is the other half of singular extensions — when the
verification search shows that multiple moves beat the singular beta
at reduced depth, the entire subtree is suspect.  Return beta
immediately rather than searching all moves.

This is already implemented as part of 6.1 (step 5, branch 1).  The
refinement is tuning the conditions: make sure multi-cut only fires
when the reduced search is genuinely reliable (enough depth, enough
moves searched).

**SPRT validation:** Expected +3 to +8 Elo on top of singular extensions.

### 6.3 — Threat extensions

**After SE passes SPRT:**

When the previous move was heavily reduced (R ≥ 3 plies) and the
opponent's eval is not declining, the opponent may have missed a
threat.  Extend the current node by one ply to ensure the threat is
seen.

This comes from Berserk's `(ss-1)->reduction >= 3` check.  It is also
present in Stockfish's hindsight adjustment and Reckless's threat
compensation.

**What to build:**
```
if (ss-1).lmr_reduction >= 3
   && !improving  // opponent's eval not declining
   && depth >= 2 {
    depth += 1;
}
```

**SPRT validation:** Expected +3 to +8 Elo.

### 6.4 — Recapture extensions

**After SE passes SPRT:**

In PV nodes, when a capture is made on the same square as the previous
move's capture (a recapture), extend by one ply.  These are critical
tactical sequences that deserve deeper search.

Present in Caissa, Marvin, and Stockfish.

**What to build:**
```
if is_pv && !is_root && is_capture
   && move_to(current_move) == move_to(prev_move)
   && prev_move_was_capture {
    extension += 1;
}
```

**SPRT validation:** Expected +2 to +5 Elo.  Small, situational gain.

---

## Section 7 — Layer 5: Time

**Purpose:** Allocate time intelligently based on what the search
reveals about the position.  Spend more time when the position is
critical, the score is unstable, or the best move is unclear.

**Dependencies:** Layers 0–4.  Time management can only be trusted
when the search results are trustworthy.  Spending more time on a
bad evaluation of a critical position is worse than playing quickly.

### 7.1 — PV stability factor

**What it is:** Track how many consecutive iterations have returned the
same best move.  When the PV is stable (same move for many depths),
the engine is confident and can play earlier.

**What to build:**
```
stability = same_best_move ? min(10, stability + 1) : 0
stability_factor = 1.3 - 0.05 * stability
// Range: 1.30 (unstable, just changed) to 0.80 (stable for 10 depths)
```

At stability 0: 1.30× base time (more time, position is changing).
At stability 10: 0.80× base time (less time, we're confident).

Every top engine except Marvin has this.

### 7.2 — Score trend factor

**What it is:** Track how the root score changes between iterations.
A dropping score (we're getting worse) means the position is critical
and needs more time.  A rising or stable score means we can play
earlier.

**What to build:**
```
score_delta = prev_score - current_score
score_factor = clamp(0.75, 0.75 + 0.05 * score_delta / 100, 1.5)
// Range: 0.75 (winning, play fast) to 1.50 (losing, think harder)
```

At score delta 0: 0.75× base time (stable, no urgency).
At score delta +1000: 1.25× base time (dropping fast, danger).
At score delta -500: 0.75× base time (improving, fine to play).

Every top engine except Marvin has some version of this.

### 7.3 — Node distribution factor

**What it is:** If most nodes are spent on a single move, the engine
is confident and can play earlier.  If nodes are split across
multiple candidates, the position is complex and needs more time.

**What to build:**
```
not_best_pct = 1.0 - (best_move_nodes / total_nodes)
node_factor = 0.55 + 2.0 * not_best_pct
// Range: 0.55 (unanimous, play fast) to 2.55 (split, think harder)
```

At 90% on best move: 0.55 + 2.0 × 0.10 = 0.75× base time.
At 40% on best move: 0.55 + 2.0 × 0.60 = 1.75× base time.

Every top engine except Marvin has this.

### 7.4 — Combined time modulation

Combine the three factors multiplicatively:

```
adjusted_time = base_time * stability_factor * score_factor * node_factor
```

Clamp to reasonable bounds (never exceed hard limit, never go below
10ms minimum).  The hard limit remains `base_time * 5` as a ceiling.

### 7.5 — Easy-move detection

**What it is:** When the best move is far ahead of all alternatives
and the score is stable, play it early — even if time remains.

This emerges naturally from the stability and node factors.  When the
same move is returned at every depth AND 90%+ of nodes are on that
move, both factors drop below 0.80, and the engine stops early.

### 7.6 — Node-time estimation

**What it is:** Before starting iteration N+1, estimate whether it
will finish within the remaining time budget.  If not, stop now and
return the current best move.  This prevents forfeits in time trouble.

**What to build:**
```
estimated_time = prev_iteration_time * branching_factor
if elapsed + estimated_time > hard_limit {
    break;  // don't start next iteration
}
```

Where `branching_factor` is the ratio of nodes at the current depth
to nodes at the previous depth.  Typically ~2–4 for a chess engine.

7 of 8 top engines have this.

### 7.7 — Time management observability

**Before SPRT or match testing:**

Time management cannot be SPRT-tested in the normal way because it
doesn't change the search tree — it changes when the search stops.
Observability is especially critical here because the failure mode is
silent (flagging on time or playing too fast in critical positions).

**Probe events:**
- `tm_stability`: the PV stability counter at each depth
- `tm_score_trend`: the score delta and resulting factor at each depth
- `tm_node_distribution`: the best-move node fraction and resulting
  factor at each depth
- `tm_combined_factor`: the final time multiplier applied at each depth
- `tm_decision`: soft stop? hard stop? continue? at each depth
- `tm_easy_move`: was an easy-move early stop triggered?

**Match-level diagnostics (post-hoc from probe logs):**
- Time usage per move vs game phase (opening/middlegame/endgame)
- Correlation between `tm_combined_factor` and position complexity
  (using eval swing or node count as a complexity proxy)
- Time-forfeit rate (must be zero)
- Time remaining at move 40 in 40/60 games (should be > 5 seconds)

### 7.8 — Time management SPRT

Time management changes cannot be meaningfully SPRT-tested because they
don't change the search tree — they change when the search stops.  Test
via:
- Self-play matches at fixed time control (not fixed depth)
- Measure time-forfeit rate (must be zero or near-zero)
- Measure Elo gain at time controls with increment (1+0.01, 5+0.05)

---

## Section 8 — Layer 6: Evaluation

**Purpose:** Improve the static evaluation that all pruning decisions
depend on.  Do not add NNUE until the classical eval is as strong as
it can be with texel tuning.

**Dependencies:** Layers 0–5.  Better eval terms are only reliable
when the search that uses them is trustworthy.  Adding eval terms to
a search with broken pruning produces misleading SPRT results.

### 8.1 — Threat evaluation terms

Add evaluation terms for:
- Hanging pieces (undefended pieces attacked by a lower-value piece)
- Threat of fork (knight attacking two higher-value pieces)
- Threat of discovered check
- Threat of skewer/pin creation

These are the eval terms that directly interact with the extension and
pruning infrastructure built in Layers 3–4.  A position with multiple
threats is a position where singular extensions should fire — the eval
should reflect this.

### 8.2 — Passed pawn refinement

Add:
- Opposed vs unopposed passer distinction
- Connected passer bonus
- Outside passer recognition
- Enemy king distance to passed pawn (dominant factor in endgames)

### 8.3 — King safety refinement

Replace the flat `attack_units → penalty` table with piece-specific
gradations:
- Queen contact check
- Rook contact check
- Safe check (checking piece is defended)
- Number of attackers in the king zone

Stockfish distinguishes these.  They matter because queen contact is
far more dangerous than rook contact, and a safe check is worse than
an undefended one.

### 8.4 — Texel tuning

**What it is:** Optimize all eval weights simultaneously via logistic
regression on a database of labeled positions.

**What to build:**
1. Generate a training set: ~1M quiet positions from self-play games,
   labeled with game outcome (win = 1.0, draw = 0.5, loss = 0.0).
2. For each position, compute the eval as a linear combination of
   feature weights.
3. Use logistic regression to find weights that maximize the
   correlation between eval and game outcome.
4. Write the optimized weights back to the engine's constants.

This is a standard technique that gains 50–80 Elo over hand-tuned
values, without changing any search code.  Your criticality training
pipeline proves you have the ML infrastructure for this.

### 8.5 — NNUE (if desired)

**After texel tuning passes SPRT:**

NNUE (Efficiently Updatable Neural Network) replaces the classical
eval with a small neural network evaluated incrementally.  7 of 8 top
engines use NNUE.

This is a large project (2–4 weeks) but the architecture is published
(Stockfish's NNUE format) and compatible trainers exist.  The Elo
payoff is 80–150 points.

---

## Section 9 — Layer 7: Research

**Purpose:** Only here, on a trustworthy baseline, can novel ML and
search research produce interpretable results.

**Dependencies:** Layers 0–6 complete and SPRT-validated.

**Rule:** Every new idea must beat the strongest classical
implementation we can build — not the weakest one we happened to have.
The criticality LMR model must be re-tested against the new baseline.
If it still wins, the result is credible.  If it loses, the model was
compensating for weak move ordering — and that's valuable knowledge.

### 9.1 — Re-baseline criticality LMR

After Layers 0–6 are built:
1. Disable the criticality model (use pure classical LMR).
2. Run SPRT: classical LMR vs classical LMR + criticality guard.
3. If the model still gains Elo against the new baseline, it's genuine.
4. If not, the model was compensating for weak move ordering — the
   information is still valuable because it tells us what the model
   was actually doing.

### 9.2 — Re-baseline variance-aware pruning

Same process.  After Layers 0–6:
1. Disable variance-aware RFP/FFP (use classical fixed margins).
2. Run SPRT: classical vs variance-aware.
3. If variance awareness still gains Elo against the new baseline,
   the diffusive model is genuinely useful.
4. If not, the model was compensating for inaccurate eval — the
   correction history (Layer 1.5) and texel tuning (Layer 6.4) made
   the eval accurate enough that fixed margins work fine.

### 9.3 — Future research directions

With a trustworthy baseline, genuinely new ideas become testable:
- Per-move uncertainty estimation (instead of per-node σ)
- Learned LMR formulas (not just a guard, but the reduction itself)
- Dynamic depth allocation based on position complexity
- Adversarial search (Lc0-style search adapted to classical engines)

These are the ideas that can make Boa distinctive.  But they need the
baseline to be credible first.

---

## Section 10 — Boa's Strategic Assets

These are the things that make Boa different from every other engine.
They must be preserved and extended as the engine grows.

### 10.0 — The probe-first architecture

Boa was built instrumentation-first, not search-first.  Most engines
add measurement grudgingly when something breaks.  Boa was built around
measurement from the start.

This means: every subsystem ships with observability before optimization.
The probe events are not an afterthought — they are the acceptance
criteria.  A subsystem without probes is invisible.  An invisible
subsystem cannot be debugged, cannot be tuned, and cannot be trusted.

The standard is: before any subsystem is considered complete, you must
be able to answer from probe data:
- How often does it fire?
- What does it contribute?
- Was it right when it fired?
- Is it saturated or under-utilized?
- Is it learning or static?

If any of these questions cannot be answered, the subsystem is not done.

### 10.1 — The probe system

Boa's probe system is more comprehensive than anything in any top
engine.  25+ event types covering every decision point, with
token-efficient field names, per-event sampling rates, and
self-describing JSONL output.

**Preservation rule:** Every new module added to the engine MUST add
probe events for its key decision points.  The probe system is not
optional infrastructure — it is the engine's sensory nervous system.

### 10.2 — The experiment log

`EXPERIMENTS.md` records every experiment, result, and lesson.  Failed
branches are documented, not deleted.  This discipline is rarer than
it sounds and is a strategic advantage.

**Preservation rule:** Every SPRT result, every rejected idea, every
architectural lesson goes into `EXPERIMENTS.md`.  The document is the
engine's institutional memory.

### 10.3 — The counterfactual pipeline

Shadow-only training with unbiased probes is methodologically correct.
The criticality pipeline (`tools/train.py`, shadow counterfactual
probes, P97 threshold) is built on sound statistical principles.

**Preservation rule:** When the pipeline is generalized beyond
criticality (for texel tuning, NNUE training, etc.), maintain the
same methodological rigor — unbiased sampling, explicit provenance,
versioned outputs.

### 10.4 — The measurement-first culture

Boa was built instrumentation-first, not search-first.  Most engines
add measurement grudgingly when something breaks.  Boa was built
around measurement from the start.  This is the strategic differentiator.

**Preservation rule:** Every architectural decision, every new
heuristic, every tuning change must be accompanied by the measurement
that justifies it.  "It feels right" is never sufficient.

---

## Section 11 — The Boa vs Top 8 Gap Map

| # | Subsystem | Boa | Top 8 | Priority |
|---|-----------|-----|-------|----------|
| 1 | SEE pin exclusion | Missing | Universal | **Layer 0** |
| 2 | Gravity history | Overflow-based | Gravity (8/8) | **Layer 0** |
| 3 | TT raw eval storage | Missing | 7/8 have it | **Layer 0** |
| 4 | Non-zero history init | Missing | Universal | **Layer 0** |
| 5 | Continuation history | Missing | 8/8 | **Layer 1** |
| 6 | Pawn history | Missing | 5/8 | **Layer 1** |
| 7 | Correction history | Missing | 6/8 | **Layer 1** |
| 8 | Clustered TT | Direct-mapped | 8/8 clustered | **Layer 2** |
| 9 | TT in qsearch | Missing | 7/8 | **Layer 2** |
| 10 | Verified NMP | Missing | 6/8 | **Layer 3** |
| 11 | ProbCut | Missing | 8/8 | **Layer 3** |
| 12 | Razoring | Missing | 8/8 | **Layer 3** |
| 13 | LMR 10+ adjustment terms | 2-3 terms | 10+ terms | **Layer 3** |
| 14 | Singular extensions | Missing | 8/8 | **Layer 4** |
| 15 | Multi-cut | Missing | 7/8 | **Layer 4** |
| 16 | Threat/recapture ext | Missing | 4/8 each | **Layer 4** |
| 17 | Dynamic time mgmt | Missing | 7/8 | **Layer 5** |
| 18 | Eval terms (threat, etc.) | Missing | Universal | **Layer 6** |
| 19 | Texel tuning | Missing | Universal | **Layer 6** |
| 20 | NNUE | Missing | 7/8 | **Layer 6 (optional)** |

---

## Section 12 — Testing Methodology

### 12.1 — The SPRT standard

Every search/eval change must pass SPRT at fast time control:
- `cutechess-cli` with `-sprt elo0=0 elo1=5 alpha=0.05 beta=0.05`
- Time control: 1.0+0.01s or similar
- Opening book: diverse set of 200+ positions (not just startpos)
- Hash: 16 MB, Threads: 1

### 12.2 — What does NOT need SPRT

- Correctness fixes (SEE pins): SPRT only to confirm no regression
- Infrastructure changes (TT raw eval, gravity history): `cargo test` +
  bench comparison is sufficient
- Code quality (clippy, formatting, refactoring): `cargo test`

### 12.3 — Opening book diversity

Self-play-only testing from startpos teaches the engine to beat itself.
Use a diverse opening book with 200+ positions covering different pawn
structures, material balances, and game phases.  `tools/openings.epd`
exists — expand it if needed.

### 12.4 — One change per SPRT

Never bundle multiple changes in a single SPRT.  The "root conversion
tie-break" bug (incorrectly applied bonus) was caught because you ran
it alone.  If you'd bundled it with other changes, the bug would have
been attributed to the bundle, not the specific change.

---

## Section 13 — Implementation Sequencing

The build order within each layer:

### Layer 0 (immediate, no SPRT required to justify)

1. SEE pin exclusion
2. Gravity-based history updates
3. Non-zero history initialization
4. TT raw eval storage

### Layer 1 (build incrementally, SPRT each step)

0. History bonus formula upgrade (3.0) → SPRT (test before continuation history)
1. Continuation history 1-ply (3.1) → SPRT
2. Continuation history 2-ply (3.2) → SPRT
3. Continuation history 4,6-ply (3.3) → SPRT
4. Pawn history (3.4) → SPRT
5. Correction history (3.5) → SPRT

### Layer 2 (after Layer 1 SPRT passes)

1. Clustered TT → SPRT
2. TT qsearch probe + store → SPRT
3. Replacement policy refinement → SPRT

### Layer 3 (after Layers 1+2 SPRT pass)

1. Verified NMP → SPRT
2. Razoring → SPRT
3. ProbCut → SPRT
4. LMR refinement (one term at a time) → SPRT each

### Layer 4 (after Layer 3 SPRT passes)

1. Singular extensions (single-level) → SPRT
2. Multi-cut (already in SE code, verify) → SPRT
3. Threat extensions → SPRT
4. Recapture extensions → SPRT

### Layer 5 (after Layer 4 SPRT passes)

1. PV stability factor
2. Score trend factor
3. Node distribution factor
4. Combined modulation + easy move
5. Node-time estimation

### Layer 6 (after Layer 5 complete)

1. Threat eval terms → SPRT
2. Passed pawn refinement → SPRT
3. King safety refinement → SPRT
4. Texel tuning → SPRT
5. NNUE (if desired) → SPRT

### Layer 7 (after Layers 0–6 SPRT-validated)

1. Re-baseline criticality LMR → SPRT against classical LMR
2. Re-baseline variance-aware pruning → SPRT against classical FFP/RFP
3. New research → SPRT against strongest classical implementation

---

## Section 14 — Reusable Infrastructure

### 14.1 — Training pipeline architecture

Boa's criticality training pipeline (`tools/train.py`, shadow
counterfactual probes, P97 threshold) is built on reusable primitives.
Future ML projects (texel tuning, NNUE training, learned LMR formulas)
should inherit this architecture rather than building from scratch.

**Reusable components:**

1. **Data collection.**  Self-play games with probes enabled produce
   JSONL files.  The game runner (`tools/criticality_dataset.mjs`)
   handles engine invocation, game pairing, and result collection.
   Future data collection for texel tuning or NNUE training uses the
   same pattern: engine self-play with probe output.

2. **Probe format.**  The self-describing JSONL format (meta header
   with field legend, one event per line, token-efficient field codes)
   is documented in `docs/superpowers/specs/2026-06-29-probe-system-design.md`.
   Any new training task adds event types following the same convention.

3. **Feature extraction.**  `tools/train.py` reads JSONL, filters by
   event type, extracts features, and normalizes.  The `FEATURES` list
   at the top of `train.py` is the single source of truth for the
   criticality feature set.  A texel tuning pipeline would follow the
   same pattern: a `FEATURES` list as the source of truth, a CSV/JSONL
   reader, and a feature normalizer.

4. **Training.**  Logistic regression with scikit-learn, P97 threshold
   selection from validation-score percentiles.  The output is a
   versioned `.coeffs` file with the history separator (`---`).

5. **Versioning.**  Previous coefficients are archived as commented
   blocks below the `---` separator — never parsed, never lost.  This
   pattern applies to any future ML output (texel weights, NNUE
   networks, learned LMR tables).

6. **Validation.**  `tools/train.py check` prints probe health summary
   (row counts, label distribution, feature coverage).  Future training
   tasks should have an equivalent health check.

### 14.2 — Opening book construction

The opening book used for SPRT testing must be diverse enough that
self-play results are not biased by the starting position.

**Source:** Start from `tools/openings.epd`.  Expand if needed.

**Diversity goals:**
- 200+ unique positions
- Cover all major pawn structures (open, semi-open, closed, isolated
  queen pawn, King's Indian, French, Sicilian, Caro-Kann, etc.)
- Cover all game phases (opening, early middlegame)
- Include both symmetric and asymmetric positions
- Include positions where one side has a small but clear advantage
  (~20-50 cp) to test the engine's ability to convert and defend
- No position should be a known forced draw or forced win within 10 ply

**Source positions:** The standard approach is to take positions from
high-level human or engine games after 8-12 moves of established opening
theory.  This ensures the positions are natural and testing quality.

**Maintenance:** When a new SPRT testing campaign begins, verify that
the opening book still meets the diversity criteria.  If the engine has
become significantly stronger, some positions may have become trivial
— replace them.

**Acceptance criteria:** A position is accepted if both the baseline
and candidate engines find a non-losing move within 100ms at the book
position.  Positions where either engine blunders immediately are
excluded.

### 14.3 — TT prefetch

On modern CPUs, TT probe latency is a bottleneck.  A software prefetch
instruction issued before `make_move()` can hide 20-50 cycles of memory
latency.

**What to add:** In `alpha_beta()`, before making a move, compute the
hash of the resulting position and issue a prefetch:

```rust
// Before make_move:
let child_hash = board.hash_after_move(m, ctx.z);
ctx.tt.prefetch(child_hash);
let undo = board.make_move(m, ctx.z);
// ... recursive search ...
```

The `prefetch()` method calls `_mm_prefetch` on x86_64 or
`__builtin_prefetch` via std::intrinsics::prefetch_read_data on stable
Rust.  No-op on architectures without prefetch support.

**Verification:** `bench` should show a small NPS increase (2-5%).
Not SPRT-tested — this is a performance optimization, not a search
change.

### 14.4 — hashfull with clustered TT

The current `hashfull` formula counts entries with a matching age in
the first 1000 slots.  With clustered TT (Layer 2, section 4.1), the
formula changes:

```
hashfull = (entries_with_matching_age_in_first_1000_buckets * 1000)
           / (1000 * ENTRIES_PER_BUCKET)
```

Each bucket has 3 entries.  Count all 3, not just the first.  Divide
by 3000 instead of 1000.  This produces a value in 0-1000 (permille)
that reflects actual table utilization.

Implementation: scan the first 1000 buckets.  For each bucket, count
how many of the 3 entries have an age matching the current search.
Sum across all 1000 buckets.  Return `sum * 1000 / 3000`.

---

## Section 15 — Time Management Architecture

The time management formulas are specified in Layer 5 (Section 7).
This section specifies where the state lives.

### 15.1 — State ownership

**Per-search state (in `SearchContext` or the iterative deepening loop):**

```
pv_stability: u32          // consecutive iterations with same best move
prev_best_move: Move       // best move from previous depth
prev_score: Score          // root score from previous depth
best_move_nodes: u64       // nodes spent on the current best move
total_nodes_this_search: u64  // total nodes across all root moves this search
```

These are local to a single search (one UCI `go` command).  They do
not persist across searches.

**Per-search state (in the time manager):**

```
base_time: u64             // computed once at search start from clock + increment
hard_limit: u64            // never-exceed ceiling
optimum_time: u64          // before modulation
```

These are computed in `time_for_move()` and stored in the search
context or limits.

### 15.2 — When state is updated

- `pv_stability`: incremented at the end of each depth iteration
  if `best_move == prev_best_move`, reset to 0 otherwise.
- `prev_best_move`: set at the end of each depth iteration.
- `prev_score`: set at the end of each depth iteration.
- `best_move_nodes`: accumulated during the search (root move node
  tracking already exists in `root.rs` via `rootMoves`).
- `total_nodes_this_search`: `ctx.nodes` at the end of each depth.

### 15.3 — When time is checked

- **Between iterations:** After each depth completes, compute the
  combined modulation factor and check `elapsed >= optimum_time * factor`.
  If so, stop.
- **During iteration:** Every 2048 nodes (or similar cadence), check
  `elapsed >= hard_limit`.  If so, stop immediately.
- **Before starting iteration N+1:** Estimate whether `elapsed +
  estimated_next_iteration_time >= hard_limit`.  If so, don't start
  the next iteration.

---

## Section 16 — Layer 6 Scope Boundary

Layer 6 (Evaluation) is a roadmap, not an implementation specification.
Each item — threat terms, passed pawn refinement, king safety
refinement, texel tuning, NNUE — will receive its own dedicated design
document before implementation begins.

This document specifies:
- That these items exist and their position in the dependency order
- That texel tuning comes before NNUE
- That eval terms are only reliable when the search that uses them is
  trustworthy (Layers 0-5)

It does not specify:
- Exact eval term definitions or weight ranges
- Training data pipeline details for texel tuning
- NNUE architecture or training methodology

The evaluation specification will be written when Layer 5 is complete
and SPRT-validated.  Until then, this section serves as a placeholder
in the dependency graph.

---

### A.1 — Continuation history offsets by engine

| Engine | Offsets used | Update weights |
|--------|-------------|----------------|
| Stockfish | 1,2,3,4,5,6 | `{1:1040, 2:780, 3:300, 4:537, 5:129, 6:423}` / 1024 |
| Reckless | 1,2,4,6 | Full at 1, full at 2, half at 4, half at 6 |
| PlentyChess | 1,2,4,6 | Full/Full/Half/Half |
| Obsidian | 1,2,4,6 | Full/Full/Half/Half |
| Ethereal | 1,2 (CM+FM) | Full/Full |
| Berserk | 1,2,4,6 | `{1:1014, 2:300, 3:978, 5:978}` / 1024 |
| Marvin | 1,2 | Full/Full |
| Caissa | 0,1,3,5 | `{1:1014, 2:300, 3:978, 5:978}` / 1024 |

### A.2 — History bonus formulas

| Engine | Quiet history bonus |
|--------|--------------------|
| Stockfish | `min(134*d - 79, 1572)` |
| Obsidian | `min(175*d + 15, 1409)` |
| Ethereal | `min(1708, 4*d² + 191*d - 118)` |
| Berserk | `min(1708, 4*d² + 191*d - 118)` |
| Caissa | `min(-113 + 164*d + 148*scoreDiff/64, 2178)` |

### A.3 — ProbCut margins

| Engine | Margin |
|--------|--------|
| Stockfish | `214 - 59*improving` |
| Berserk | 168 |
| Caissa | 133 |
| PlentyChess | 214 |
| Ethereal | 100 |
| Obsidian | 190 |

### A.4 — NMP reduction formulas

| Engine | Formula |
|--------|---------|
| Stockfish | `R = 7 + depth/3` |
| Reckless | `R = (4407 + 917*improving + 265*depth + 477*eval_above_beta/128) / 1024` |
| PlentyChess | `R = 351 + 100*depth/243 + min(100*(eval-beta)/211, 400)` |
| Obsidian | `R = depth/3 + (eval-beta)/147 + 4` |
| Ethereal | `R = 4 + depth/5 + min(3, (eval-beta)/191)` |
| Berserk | `R = 4 + 367*depth/1024 + min(9*(eval-beta)/1024, 4)` |

---

## Appendix B — What Boa Must NOT Become

**Do not become a Stockfish clone.**  Stockfish is the result of two
decades of work by dozens of people with thousands of CPU-years of
testing.  Copying it feature-for-feature is not possible for a solo
project and would produce a second-rate copy.

**Do not abandon the measurement-first approach.**  Boa's probes,
experiment log, and counterfactual pipeline are strategic assets.  Most
engines evolve search-first and bolt on measurement later.  Boa is the
inverse — and that is the thing that makes genuinely novel research
possible in the long run.

**Do not skip layers.**  Every layer physically depends on the one
beneath it.  Building Layer 7 research on a weak Layer 0–1 foundation
produces results that are not interpretable.  The criticality model
beat a weak baseline.  Whether it beats a strong baseline is a
different question — and that question is worth answering correctly.

**Do not test multiple changes at once.**  The SPRT is only as
trustworthy as the experimental design.  One variable per test.

---

## Appendix C — The Metric

The primary metric is SPRT Elo at fast time control (1+0.01 or similar).
Internal metrics (AUC, RMSE, Pearson correlation, node counts, TT hit
rate, pruning accuracy) are diagnostics.  They do not substitute for
playing-strength validation.

A change that improves a diagnostic but loses Elo is wrong.
A change that gains Elo but degrades a diagnostic is right — and the
diagnostic was measuring the wrong thing.

The probe system exists to turn diagnostics into actionable insights.
Use probes to understand *why* a change gained or lost Elo, not to
decide *whether* it gained or lost Elo.
