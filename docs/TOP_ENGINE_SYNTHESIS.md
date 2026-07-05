# Top 8 Chess Engines — Search Architecture Synthesis

Bone-deep analysis of Stockfish, Reckless, PlentyChess, Obsidian, Ethereal,
Berserk, Marvin, Caissa. All except their evals.

---

## 1. Move Ordering & History

### What EVERY engine has:

**Basic history tables:**
- Butterfly/quiet history: `[color][piece][to]` or `[from_to]` — every engine
- Capture history: `[piece][to][captured_type]` — every engine
- Killer moves: 1 or 2 per ply — every engine
- Counter-move heuristic: `[prev_piece][prev_to] → move` — every engine

**Continuation history — every single engine:**
Every top engine has continuation history. This is not optional. It is the single
biggest missing item in Boa.

| Engine | Continuation offsets used | Table dimensions |
|--------|--------------------------|------------------|
| **Stockfish** | 1, 2, 3, 4, 5, 6 | `[piece][to][piece][to]` per offset |
| **Reckless** | 1, 2, 4, 6 | `[piece][to][piece][to]` per offset |
| **PlentyChess** | 1, 2, 4, 6 | `[piece][to][piece][to]` per offset |
| **Obsidian** | 1, 2, 4, 6 | `[piece][to][piece][to]` per offset |
| **Ethereal** | 1, 2 (CM + FM) | `[piece][to][piece][to]` × 2 |
| **Berserk** | 1, 2, 4, 6 | `[piece][to][piece][to]` per offset |
| **Marvin** | 1, 2 (counter + follow) | `[piece][to][piece][to]` × 2 |
| **Caissa** | 0, 1, 3, 5 | `[piece][to][piece][to]` per offset |

**History update formula — universal:**
```
new = old + delta - old * |delta| / GRAVITY
```
Every engine uses this. Gravity constants:
- Stockfish: 65536 (D=65536)
- Most others: 16384 (D=16384)
- Caissa: 16384

**History initialization — every engine:**
All initialize with slight negative values (typically -5 to -700 range),
not zero. This biases against unproven moves.

**Pawn history — 5 of 8 engines:**
Stockfish, Reckless, PlentyChess, Obsidian, Berserk all have a pawn-keyed
history table. Marvin, Ethereal, Caissa do not.

### Unique move ordering ideas:

- **Obsidian's threat-based scoring**: +/-32768 per piece type for moving
  threatened pieces or moving into threatened squares
- **Reckless's eval-based quiet history bonus**: applies a hindsight bonus
  to the previous quiet move based on eval change: `812 * (-(eval + prev_eval)) / 128`
- **Reckless's pawn-wall penalty in quiet scoring**: -4494 for moving pawns
  from the pawn wall
- **Caissa's prior CMH update**: applies continuation history bonuses even
  in fail-low nodes — learning from what the opponent did right
- **Caissa's threat-conditioned butterfly**: `[stm][from_threat][to_threat][from_to]`
  — 2 extra threat bits in the key
- **Berserk's CH update with alternating weight patterns**: offset 0=full,
  1=1014/1024, 2=300/1024, 3=978/1024, 5=978/1024 (deliberately skips some offsets)
- **Stockfish's skip of contHist[4]** in scoring but use of contHist[5] — a
  deliberate asymmetry found through tuning

### Boa's gap:
You have: butterfly, counter-move, capture history, killers (2 slots).
You're missing: continuation history (all 8 engines have it), pawn history (5/8
have it).

---

## 2. History Heuristic Bonus Formulas

### Depth-scaled bonus — universal:
Every engine scales the history bonus by depth. The bonus applied to the best
move on a beta cutoff grows with search depth:

| Engine | Formula |
|--------|---------|
| Stockfish | `min(134*d - 79, 1572)` |
| Reckless | `clamp(148*depth*diff/128, -4678, 2496)` (for correction) |
| PlentyChess | `min(139 + 269*d, 2083)` |
| Obsidian | `min(175*d + 15, 1409)` |
| Ethereal | `min(1708, 4*d² + 191*d - 118)` |
| Berserk | `min(1708, 4*d² + 191*d - 118)` |
| Marvin | `32*d²` |
| Caissa | `min(-113 + 164*d + 148*scoreDiff/64, 2178)` |

The depth is sometimes boosted: `depth + (bestScore > beta + threshold)`
increases the bonus when the fail-high is especially strong.

### History aging/decay:
All engines decay history between searches (or between games):
- **PlentyChess**: multiply all quiet history by 3/4 before each ID iteration
- **Caissa**: divide all tables by 2 on `NewSearch()`
- **Stockfish/Obsidian/Reckless**: rely on gravity-based natural decay
- **Berserk/Ethereal**: gravity-based only

### Boa's gap:
Your bonus formula is `depth²` (standard but basic). Your history decays via
overflow-triggered division by 2. Modern engines use gravity-based decay
exclusively (no overflow threshold needed).

---

## 3. Pruning — The Standard Stack

### What EVERY engine has:

**Null-move pruning (NMP):**
Every engine. Universal. The standard formula is `R = 3-4 + depth/3-6`.
The eval-above-beta term (more reduction when eval is far above beta) is
near-universal:
- Stockfish: `R = 7 + depth/3`
- Berserk: `R = 4 + 367*depth/1024 + min(9*(eval-beta)/1024, 4)`
- PlentyChess: `R = 351 + depth/243 + min((eval-beta)/211, 400)` (in depth/100 units)
- Ethereal: `R = 4 + depth/5 + min(3, (eval-beta)/191)`
- Obsidian: `R = depth/3 + (eval-beta)/147 + 4`

**Reverse futility pruning (RFP):**
Every engine. Margin = linear/quadratic in depth, with improving/not-improving split.
Typically `margin = k * depth` where k ranges from 40-90. Depth limit typically
6-11 for RFP, deeper for NMP.

**Forward futility pruning (FFP):**
Every engine. Applied to quiet moves at the frontier (shallow remaining depth).
Margin typically `base + factor * depth`.

**Razoring:**
Every engine. At depth 1-4, if eval is far below alpha, do a qsearch and
potentially return early.

**ProbCut:**
Every engine except Marvin (which doesn't have it). Standard approach:
`probBeta = beta + margin`, search captures with SEE ≥ threshold at reduced depth.
- Stockfish margin: `214 - 59*improving`
- Berserk margin: `168`
- Caissa margin: `133`
- PlentyChess margin: `214`

**Delta pruning in qsearch:**
Every engine except Marvin (which has no delta pruning). Standard margin:
- Ethereal: `QSDeltaMargin = 142`
- Obsidian: `QsFpMargin = 156`
- Berserk: `futility = bestScore + 79`
- Stockfish: `futilityBase = ss->staticEval + 335`

**SEE pruning:**
Every engine. Applied in both main search and qsearch.

### Verified NMP — most have it:
- **Stockfish**: yes, verification at `depth >= 16`
- **Reckless**: yes, verification at `depth >= 16`
- **PlentyChess**: yes, verification at `depth >= 1500` (= 15 ply)
- **Berserk**: yes, recursive NMP with `nmpMinPly` tracking
- **Ethereal/Obsidian**: no explicit verification search
- **Marvin**: yes, `try_null=false` in child prevents double-null
- **Caissa**: yes, verification at `depth < NmpReSearchMaxDepth(10)`

### Unique pruning ideas:

- **Reckless's RFP lerp**: returns `lerp(eval, beta, 0.6945)` instead of `eval`
- **Reckless's correction value everywhere**: NMP, RFP, futility, and SEE
  margins all include the correction value
- **Caissa's in-check ProbCut**: a separate ProbCut pathway when in check
- **Stockfish's secondary aging**: TT entries with decisive non-exact scores
  get depth decayed by 1 on write rejection
- **Obsidian's TT cut-node verification**: penalizes continuation history when
  a cut-node returns from TT cutoff
- **PlentyChess's aspiration reduction**: on repeated fail-highs, reduces search
  depth rather than just widening the window

### Boa's gap:
You have: NMP (unverified), RFP (variance-aware), FFP (variance-aware),
delta pruning, SEE pruning. Your NMP lacks verification. You lack ProbCut
entirely. Your variance-aware FFP/RFP is genuinely novel — no top engine does
it this way — but it's compensating for a weak baseline rather than adding to
a strong one.

---

## 4. Extensions

### What EVERY engine has:

**Singular extensions:**
Every engine. This is the single biggest Elo source among extensions.
The standard pattern:
- Trigger: `depth >= 6-8`, `move == ttMove`, `ttDepth >= depth - 3`
- Reduced verification search: `(depth-1)/2` or `depth/2`
- Singular margin: `ttScore - depth` or similar
- Extension: 0-3 plies depending on how singular

| Engine | Max extension | Negative extension? |
|--------|--------------|---------------------|
| Stockfish | 3 | Yes (-2, -3) |
| Reckless | 3 | Yes (-3) |
| PlentyChess | 3 | Yes (-2, -3) |
| Obsidian | 3 | Yes (-2, -3) |
| Ethereal | 2 | Yes (-1) |
| Berserk | 3 | Yes (-2, -1) |
| Marvin | 1 | No |
| Caissa | 3 | Yes (-2, -1) |

**Multi-cut:**
Every engine with singular extensions. If the singular verification search
shows the TT move is NOT singular AND `singularBeta >= beta`, return beta
immediately.

**Check extension:**
Every engine. Some explicit (Marvin: +1), some implicit through reduced LMR
(Obsidian: `R -= 1` for checks, Reckless: `R -= 955/1024` for checks).

### Unique extension ideas:

- **Ethereal's double extension counter**: capped at 6 per line (`dextensions <= 6`)
- **Reckless's correction-dependent singular margins**: the double/triple margins
  include `|correction_value|` terms
- **Stockfish's hindsight adjustment**: based on prior reduction + eval delta
- **Reckless's hindsight reduction compensation**: `reduction >= 2249` (2.2 ply)
  + eval worsened → extend
- **Berserk's threat detection via sibling LMR**: parent checks `(ss-1)->reduction >= 3`
  to detect missed threats
- **Marvin's recapture extension**: PV only, same-square recaptures get +1
- **Caissa's root singular search**: at ~20% of ideal time, runs a shallow
  verification at root to check if best move is singular

### Boa's gap:
You have only check extension (+1 when in check). Singular extensions do not
exist at all. This is the largest single Elo source among things you're missing.

---

## 5. Reductions (LMR)

### What EVERY engine has:
LMR with `log(depth) * log(moveIndex)` base reduction, history-adjusted.

The universal formula:
```
R = base + log(depth) * log(moves) / divisor
```

The divisor ranges from 2.0 (more aggressive) to 3.14 (more conservative):
- Stockfish: divisor = `128/28.34 ≈ 4.5` then scaled to 1/1024 units
- PlentyChess: divisor = 2.95 (quiets), 2.98 (captures)
- Obsidian: divisor = 3.14
- Ethereal: divisor = 2.47
- Berserk: divisor = 2.14
- Marvin: divisor = 2.0
- Caissa: divisor = 2.33 (quiets), 2.38 (captures)

### Universal adjustments:
Every engine adjusts LMR based on:
1. History score (good history → less reduction)
2. Improving flag (improving → less reduction)
3. Node type (PV → less reduction, cut-node → more reduction)
4. Killers/counter-moves (less reduction)
5. Check-giving moves (less reduction)

### Deeper/shallower re-search — most engines:
After the reduced search fails high or low, most engines adjust the re-search
depth:
- Stockfish: `newDepth++` if `value > bestValue + 52`, `newDepth--` if `value < bestValue + 9`
- Reckless: `newDepth += (score > best_score + 57)`, `newDepth -= (score < best_score + 9)`
- Obsidian: `newDepth += (score > bestScore + 43 + 2*newDepth)`, `newDepth -= (score < bestScore + 11)`
- Caissa: `newDepth++` if `score > bestValue + 85`
- Ethereal: same pattern
- Berserk: `newDepth += (score > bestScore + 61)`

### Unique reduction ideas:

- **Stockfish's LMR divisor table**: 16-entry table per depth for
  continuation-history-based depth adjustment. `lmrDepth += history / lmrDivisor[dIndex]`
- **Reckless's two-tier LMR**: separate formulas for `depth >= 2` vs full-depth
  search
- **Reckless's randomization**: `+ ((nodes + id*27) % 128) - 59` — deliberate
  noise for search diversity
- **Obsidian's complexity-based LMR**: `R -= ss->complexity / 120` where
  complexity = |staticEval - rawStaticEval|
- **Reckless's correction value in LMR**: `- 3417 * |correction| / 1024`
- **PlentyChess's early LMR**: a pre-computed LMR used before the actual search
  to estimate `lmrDepth` for futility/history pruning decisions

### Boa's gap:
You have LMR with criticality guard. The criticality model adds protection for
~3% of reduced moves. But your base LMR formula (`log * log / divisor`) is
coarse compared to what every top engine has: 10+ individual adjustment terms
per move, continuation-history-based reduction scaling, and adaptive re-search
depth.

---

## 6. Transposition Table

### What EVERY engine has:

**Clustered buckets:**
Every engine uses set-associative (clustered) TT, NOT direct-mapped:
| Engine | Entries per bucket | Bucket size |
|--------|-------------------|-------------|
| Stockfish | 3 | 32 bytes |
| Reckless | 3 | 32 bytes |
| PlentyChess | 5 | 64 bytes |
| Obsidian | 3 | 32 bytes |
| Ethereal | 3 | 32 bytes |
| Berserk | 3 | 32 bytes |
| Marvin | 3 | 64 bytes |
| Caissa | 3 | 32 bytes |

Every single engine uses 3 entries per bucket (PlentyChess uses 5). Boa uses
direct-mapped (1 entry per bucket). This means Boa has 2-3× the collision rate
at the same table size.

**TT in qsearch:**
Every engine probes the TT in quiescence search. Most also store. The only
exception is Marvin which probes but does not store.

**Raw eval in TT:**
Every engine except Marvin stores the static eval in the TT entry, enabling
eval reuse without recomputation.

**Age-based replacement:**
Every engine uses generation/age bits in the TT entry. Replacement prefers
entries with lower depth and older age. The standard formula is `depth - k*age`.

**Hash indexing via multiplication:**
Every engine uses `(hash * bucketCount) >> 64` (128-bit multiply) for fast
hash→index mapping, avoiding modulo.

### Unique TT ideas:

- **Stockfish's TT cutoff verification**: for `depth >= 7`, plays the TT move
  and verifies the child TT entry matches — defense against GHI bugs
- **Stockfish's secondary decay**: non-exact entries with decisive scores get
  depth decayed by 1 on write rejection (important for elementary mates)
- **Reckless's SIMD key comparison**: uses bit-parallel `0x0001_0001_0001_0001`
  broadcast to check all 3 keys in one operation
- **Marvin's date in move field**: packs 8-bit date into unused upper 10 bits
  of the 32-bit move field, saving 2 bytes per entry
- **Caissa's 32-byte clusters**: half cache line — unusual choice
- **Obsidian's quality-based replacement**: `quality = depth - 8*age` (unusually
  high age penalty)
- **Berserk's packed eval+move**: packs 12-bit eval and 20-bit move into one
  32-bit field

### Boa's gap:
You have: direct-mapped TT, no qsearch integration, no raw eval storage. This
is your weakest subsystem relative to the top engines.

---

## 7. Correction History / Eval Correction

### What engines have it:
- **Stockfish**: Yes. Pawn + material + continuation correction at plies -2, -4
- **Reckless**: Yes. Pawn + non-pawn(White) + non-pawn(Black) + continuation at -2, -4
- **PlentyChess**: Yes (shared per NUMA node)
- **Obsidian**: Yes. Pawn + non-pawn(White) + non-pawn(Black) + continuation
- **Ethereal**: No correction history
- **Berserk**: Yes. Pawn + continuation at -2, -3 offsets
- **Marvin**: No correction history
- **Caissa**: Yes. Pawn + non-pawn(White) + non-pawn(Black) + continuation at -2, -4

6 of 8 engines have it. The engines without it (Ethereal, Marvin) are the
weaker engines in the list.

### How it works (universal pattern):
```
correction = w1 * pawnTable[pawnHash] + w2 * nonPawnTable[key] + w3 * contTable[prev][prev2]
eval = rawEval + correction / DIVISOR
```

After search returns: `bonus = clamp((bestScore - rawEval) * depth / K, -LIMIT, +LIMIT)`
Update each table with the gravity formula.

### Impact:
Correction history debiases static eval before it feeds into RFP, NMP, and
futility margins. When eval is systematically wrong for certain position types
(closed positions, certain pawn structures), the correction compensates. This
makes every pruning decision more accurate.

### Boa's gap:
You have no correction history. This is the second biggest missing item after
continuation history. It affects everything downstream.

---

## 8. Time Management

### What EVERY engine has:

**Time allocation based on remaining clock + increment:**
Every engine. The formula `remaining/mtg + inc` is universal.

**Multi-factor time modulation:**
Every engine adjusts the base allocation based on search behavior:
1. PV stability (same move = less time)
2. Score trend (dropping score = more time)
3. Node concentration (split nodes = more time)

| Engine | Number of factors | Unique factor |
|--------|------------------|---------------|
| Stockfish | 5 | fallingEval, timeReduction, bestMoveInstability, highBestMoveEffort |
| Reckless | 5 | Nodes factor, score trend, PV stability, eval stability, BM changes |
| PlentyChess | 4 | Complexity term: `0.6 * |baseValue - currentValue| * log(depth)` |
| Obsidian | 3 | Nodes + stability + score (with tunable parameters) |
| Ethereal | 3 | PV stability, score change, node distribution |
| Berserk | 3 | Stability, score change, node count |
| Marvin | 0 | No dynamic modulation |
| Caissa | 2 | Stability + node fraction (stability *increases* time initially) |

**Hard/soft bounds:**
Every engine has a soft limit (target) and a hard limit (never exceed).

**Move overhead:**
Every engine reserves overhead (typically 10-50ms).

### Unique time management ideas:

- **Reckless's soft stop voting**: 65% thread majority required to stop,
  votes can be rescinded if conditions change
- **Reckless's 5-factor multiplier**: the most comprehensive in the list
- **PlentyChess's complexity-aware time**: scores from complex positions
  get more time
- **Stockfish's non-linear scaling formulas**: tuned for 180+1.8 and VVLTC
- **Caissa's predicted-move time adjustment**: opponent move predicted → 0.915x,
  not predicted → 1.132x

### Boa's gap:
Your time management is: `soft = remaining/mtg + inc/2`, `hard = soft*5`.
No dynamic modulation. No stability factor. No score-change factor. No
node-concentration factor. No easy-move detection. No move-criticality
allocation. This is the weakest major subsystem.

---

## 9. Quiescence Search

### What EVERY engine has:

**Stand-pat with TT probe:**
Universal.

**SEE pruning in qsearch:**
Every engine. The standard threshold is negative (allow small losses):
- Obsidian: `QsSeeMargin = -32`
- PlentyChess: `qsSeeMargin = -62`
- Ethereal: `QSSeeMargin = 123` (positive! Only winning SEE ≥ 1.23 pawns)
- Stockfish: balanced by correlation history

**Quiet checks in qsearch (first ply):**
Most engines (Stockfish, Reckless, Berserk, et al.) generate quiet checks
in the first ply of qsearch. Marvin and Ethereal do not.

**TT store in qsearch (depth 0):**
Every engine except Marvin.

### Boa's gap:
Your quiescence is not terrible but lacks TT integration entirely (the biggest
missing item) and quiet-check generation.

---

## 10. SMP / Parallel Search

### What EVERY engine uses:
**Lazy SMP** — every engine. All threads independently search the root position,
communicating only through the shared TT.

### Key design decisions:

**Shared structures:**
- **TT**: shared by ALL engines, lock-free
- **History tables**: per-thread in ALL engines
- **Correction history**: per-thread (Ethereal, Berserk, Obsidian) or per-NUMA-node (Stockfish, Reckless, Caissa, PlentyChess)

**Best thread selection:**
All engines use weighted voting: `vote += (score - minScore + C) * depth`

**NUMA awareness:**
Stockfish, Reckless, PlentyChess, and Caissa have NUMA-aware allocation.
Ethereal has basic core binding (Windows only). Obsidian, Berserk, and
Marvin do not.

### Unique SMP ideas:
- **Stockfish's NUMA-replicated NNUE**: one network copy per NUMA node
- **Reckless's soft-stop voting with rescind**: threads can change their
  vote if the position changes
- **Berserk's thread swapping**: physically swaps thread pointers to make
  the best thread the new main thread for UCI output
- **Marvin's depth staggering**: workers start at `depth = 1 + id%2`
- **Caissa's cluster-style parallelism**: threads run completely independent
  searches, no yield/steal points

### Boa's gap:
Your Lazy SMP works and is architecturally correct. Workers share the TT but
have independent history (which is standard). You lack NUMA awareness and
shared correction history (since you have no correction history). This is
not a critical gap — SMP is adequate.

---

## 11. Data Flow: How Everything Connects

The top engines share a universal architecture where every subsystem feeds
into every other:

```
Correction History → debiases Static Eval
Static Eval → feeds into RFP, NMP, FFP, razoring margins
Continuation History → sharpens Move Ordering
Better Move Ordering → more first-move cutoffs
More first-move cutoffs → more aggressive LMR is safe
LMR → uses History scores for reduction adjustment
Singular Extensions → catch what LMR would miss
ProbCut → uses qsearch+SEE for fast pre-rejection
TT → caches everything, enables transpositions
TT in qsearch → saves nodes across the majority of the tree
```

Every missing piece degrades every other piece. When Boa lacks continuation
history, the move ordering is weaker → fewer first-move cutoffs → LMR must
be more conservative → less depth → weaker play. The variance-aware pruning
is trying to compensate for positional uncertainty that sharper history
would resolve directly.

---

## 12. Boa vs Top 8: Gap Summary

| Subsystem | Boa status | Top engine standard | Gap size |
|-----------|-----------|-------------------|----------|
| Continuation history | **Missing** | 8/8 have it | **Critical** |
| Correction history | **Missing** | 6/8 have it | **Critical** |
| Singular extensions | **Missing** | 8/8 have it | **Critical** |
| Verified NMP | **Missing** | 6/8 have it | High |
| ProbCut | **Missing** | 7/8 have it | High |
| Clustered TT | **Missing** | 8/8 have it | High |
| TT in qsearch | **Missing** | 7/8 have it | High |
| TT raw eval storage | **Missing** | 7/8 have it | Medium |
| Time mgmt (dynamic) | **Missing** | 7/8 have it | Medium |
| History gravity | Overflow-based | Gravity-based (8/8) | Medium |
| Pawn history | **Missing** | 5/8 have it | Medium |
| NUMA-aware allocation | **Missing** | 4/8 have it | Low |
| LMR adjustments | 2-3 terms | 10+ terms | Low |
| Quiet checks in qs | **Missing** | 6/8 have it | Low |
| Multi-cut | **Missing** | 7/8 have it | Low |
| Thrust/recapture ext | **Missing** | 4/8 have it | Low |
