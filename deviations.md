# Architecture Implementation Deviations

Here is a summary of the implementation deviations from the `docs/superpowers/specs/2026-06-30-boa-engineering-architecture.md` specification across Layers 0 to 5.

## Layer 0: Correctness
* **Pawn History Initialization (Justified Deviation)**: The spec indicates history tables should initialize to small negative values (e.g. -5 for butterfly). `pawn_history` was initially set to `-5`, but commit `f3edff6` changed it back to `0`, noting: *"Pawn history init changed from -5 to 0 per plan spec"*. The inline comment justifies this by stating: *"pawn structure provides hard context that makes the zero-meaningful case rare — the table key is inherently informative."*

## Layer 1: Information
* **Correction History Weights (Justified Tuning)**: The spec prescribed weights `w1=30, w2=35, w3=27`. The code instead uses `3, 3, 3`. Commit `f3edff6` notes: *"Original values (30/35/27) produced average corrections of 68 cp, flipping 40% of RFP decisions and causing -69 Elo regression."*
* **Correction History 4-ply Update Dropped (Unintended?)**: The spec required `cont_corr` updates at both `ply >= 2` and `ply >= 4`. The `ply >= 4` update was initially added but silently dropped during the refactor in commit `f3edff6`.
* **Correction History Noise Guard Reverted (Unintended?)**: Commit `bf11db8` added a guard `if depth < 4 { return; }` to `update_correction` to prevent shallow tactical noise from poisoning tables (which was causing a massive node explosion). However, `f3edff6` silently reverted this guard.

## Layer 2: Memory
* **Clustered TT hashfull limit (Bug)**: The spec explicitly stated the `hashfull` algorithm must return `sum * 1000 / 3000` to yield a standard UCI per-mille (0-1000). The current implementation in `src/tt/table.rs` simply returns the raw count across 1000 buckets (returning up to 3000), violating the UCI standard limit.

## Layer 3, 4, 5
* **Fully Conformant**: No significant deviations found in Selectivity, Tactical Depth, or Time Management layers. Time Management's "easy-move" block dynamically emerges from the interaction of other factors exactly as the spec describes.