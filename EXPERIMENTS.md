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
| Root conversion tie-break, corrected implementation | -75.2 Elo +/-22.9 | Removed from `search.rs`; `candidate_root_conversion_tiebreak_fixed` vs `baseline_main_boa`, 638 games, +156 =190 -292, LOS 0%, SPRT failed at -2.95. Raw search scores drove alpha/beta and conversion only selected root moves within 8 cp, but it still badly hurt strength, especially as Black. |
| Restriction extension v2: only tighten lockdowns | -31.7 Elo +/-18.1 | Removed from `search.rs`; `candidate_restriction_extension_v2` vs `baseline_main_boa`, 1000 games, +305 =299 -396, LOS 0%, SPRT LLR -2.02. The implementation was correct, but stable lockdown extensions appear to help, especially defensively as Black. |
| Null-move squeeze threshold 8 | -56.1 Elo +/-59.7 | Removed from `search.rs`; `candidate_null_squeeze_threshold_8` vs `baseline_main_boa`, 100 games, +29 =26 -45, LOS 3.1%. Short non-SPRT run, but direction was poor and especially weak as Black, suggesting the existing null-move guard at mobility <= 12 is useful. |
| Restriction ordering positive bonus cap 400 | -31.4 Elo +/-53.7 | Removed from `search.rs`; `candidate_restriction_order_cap_400` vs `baseline_main_boa`, 100 games, +26 =39 -35, LOS 12.5%. Short non-SPRT run; capping positive restriction ordering looked negative and likely removed useful move-ordering information. |
| Advanced pawn eval ablation | -61.4 Elo +/-42.2 | Restored in `eval.rs`; `candidate_no_advanced_pawn_bonus` vs `baseline_main_boa`, 200 games, +57 =51 -92, LOS 0.2%, SPRT LLR -0.713. The standalone advanced-pawn term overlaps with PST/passers, but removing it was clearly harmful in the short run. |
| Future mobility gradient | -155.5 Elo +/-65.4 | Removed from `eval.rs`; `candidate_future_mobility_gradient` vs `baseline_main_boa`, 100 games, +18 =22 -60, LOS 0%, SPRT LLR -0.843. Gated eval-time sampling of up to three plausible quiet moves per side was expensive and tactically misleading. |
| Anti-liberty evaluation for developed pieces | -45.6 Elo +/-21.0 | Removed from `eval.rs`; `candidate_anti_liberty_developed` vs `baseline_main_boa`, stopped at 735/1000 games, +205 =229 -301, LOS 0%, SPRT LLR -2.07. Even when integrated into mobility and skipping undeveloped pieces, explicit effective-liberty penalties were harmful. |
| Quiet-check generation for quiescence | Not active | Removed from `movegen.rs`; current quiescence handles captures and bounded check evasions only. |
| Static Exchange Evaluation (SEE) helpers | Re-integrated | Initially removed; re-implemented in `search/see.rs` and now used for losing-capture pruning in quiescence and FFP see-guard. |
| Unified ML model for FFP/RFP | Failed Elo | A unified ML model across the pruning subsystem (FFP + RFP + LMR) failed to gain Elo. FFP and RFP remain classical (simple margin formulas). Do not reintroduce learned models for these without strong SPRT evidence. |
| Criticality P99 threshold for LMR protection | Replaced by P97 | The initial learned criticality LMR guard used a P99 threshold. The 200-game model tested P99, P97, and P95-r2; P97 was adopted as the best balance of protection (~3% of reduced moves) vs search overhead. |
| Criticality P95 reduction-2 gate | Not adopted | Tested on branch `criticality-200-p95-r2`: protected top 5% of reduced moves only when pre-protection reduction ≥ 2. P97 without the reduction gate was simpler and adopted instead. |

## Rejected Architectural Directions

| Experiment | Result | Notes |
| --- | ---: | --- |
| Boa squeeze philosophy (freedom metric + squeeze evaluation) | Removed in classical baseline simplification | The engine was originally built around measuring and exploiting opponent mobility restriction: freedom metric, squeeze bonuses (lockdown/severe/moderate thresholds), trade-down bonus when ahead, weak square complex, piece coordination, positional mode detection, contempt, and restriction-based move ordering. The entire subsystem was removed in commit `60ef8f4` ("Simplify engine to classical baseline"). Individual term ablations (bad bishop, space, good knight vs bad bishop, prophylaxis, minimum piece mobility) are recorded above. The lesson: classical tapered eval + narrow learned LMR protection outperformed the broad squeeze-philosophy approach. |
