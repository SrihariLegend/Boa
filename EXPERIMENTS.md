# Experiment Scratchpad

This file records engine ideas that were tried and rejected or deferred. Keep
failed experiments here instead of leaving dormant code in the engine.

## Rejected Evaluation Terms

| Experiment | Result | Notes |
| --- | ---: | --- |
| Space evaluation behind pawn chains | -53 Elo | Removed from `eval.rs`; ablation showed it hurt playing strength. |
| Bad bishop penalty | -89 Elo | Removed from `eval.rs`; consistently hurt in ablation. |
| Good knight vs bad bishop in closed centers | -53 Elo | Removed from `eval.rs`; the explicit imbalance term did not justify itself. |
| Prophylaxis pawn-break penalty | +/-0 Elo | Removed from `eval.rs`; near-zero effect and added eval complexity. |
| Minimum piece mobility penalty | -127 Elo | Removed from `eval.rs`; data-driven idea looked plausible but was harmful. |

## Rejected Move Generation/Search Utilities

| Experiment | Result | Notes |
| --- | --- | --- |
| Restriction-aware LMR from opponent mobility delta | -48.5 Elo +/-18.4 | Removed from `search.rs`; `candidate_restriction_aware_lmr` vs `baseline_main_boa`, 988 games, +282 =287 -419, LOS 0%, SPRT failed at -2.95. Reducing quiet late moves more when they released mobility and less when they preserved low mobility was too harmful. |
| Root conversion tie-break, first implementation | Invalid | Removed from `search.rs`; `candidate_root_conversion_tiebreak` vs `baseline_main_boa`, 100 games, +1 =0 -99. Implementation was buggy: the root bonus was added before root PVS/alpha handling, so null-window bounds could be promoted to exact root scores without re-search. Do not use this result as evidence against the idea. |
| Quiet-check generation for quiescence | Not active | Removed from `movegen.rs`; current quiescence handles captures and bounded check evasions only. |
| Static Exchange Evaluation (SEE) helpers | Not active | Removed from `movegen.rs`; search currently uses MVV-LVA plus capture history rather than SEE pruning/order. |
