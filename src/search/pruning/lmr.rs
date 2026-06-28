// ============================================================
// lmr.rs — Late Move Reductions with learned criticality guard
//
// This file contains both classical LMR logic and the criticality
// model inference (coefficient loading, scoring, protection decision).
// The training data pipeline (probe I/O, logging) lives in
// `src/criticality/`.
// ============================================================

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::OnceLock;

use super::super::*;

// ── Runtime-loadable criticality coefficients ────────────────────────────────
//
// The engine tries to load `criticality.coeffs` from the executable's directory
// at startup.  If the file exists and parses correctly its coefficients replace
// the hardcoded ones below.  The format is simple key=value; lines before the
// first ``---`` are active, everything after is version history (comments).
//
// The feature names in the .coeffs file must match the order used in
// `compute_score_from_coeffs`.  That order is kept in sync with the
// `FEATURES` list in `tools/train.py`.

const FEATURE_COUNT: usize = 27;

/// Canonical feature names — order must match `compute_score_from_coeffs`.
const FEATURE_NAMES: [&str; FEATURE_COUNT] = [
    "root_depth",
    "ply",
    "depth",
    "move_index",
    "base_reduction",
    "final_reduction",
    "new_depth",
    "history_score",
    "static_eval",
    "has_prev_static_eval",
    "prev_static_eval",
    "static_eval_delta",
    "alpha",
    "beta",
    "is_pv",
    "is_cut_node",
    "improving",
    "is_killer",
    "is_counter",
    "tt_move_agreement",
    "side_to_move_black",
    "piece_pawn",
    "piece_knight",
    "piece_bishop",
    "piece_rook",
    "piece_queen",
    "piece_king",
];

#[derive(Debug, Clone)]
struct CriticalityCoeffs {
    intercept: f64,
    threshold: f64,
    /// Coefficients in the canonical feature order.
    coeffs: [f64; FEATURE_COUNT],
}

static CRITICALITY_COEFFS: OnceLock<Option<CriticalityCoeffs>> = OnceLock::new();

fn get_coefficients() -> Option<&'static CriticalityCoeffs> {
    CRITICALITY_COEFFS
        .get_or_init(|| {
            let exe = std::env::current_exe().ok()?;
            let exe_dir = exe.parent()?;
            let path = exe_dir.join("criticality.coeffs");
            load_coeffs_file(&path).ok()
        })
        .as_ref()
}

fn load_coeffs_file(path: &Path) -> std::io::Result<CriticalityCoeffs> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut map: HashMap<String, f64> = HashMap::with_capacity(FEATURE_COUNT + 2);

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();

        // Stop at the history separator.
        if trimmed == "---" {
            break;
        }
        // Skip blanks and comments.
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Parse "key = value".
        if let Some((key, value_str)) = trimmed.split_once('=') {
            let key = key.trim().to_string();
            let value_str = value_str.trim();
            if let Ok(value) = value_str.parse::<f64>() {
                map.insert(key, value);
            }
        }
    }

    let intercept = map
        .remove("intercept")
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "missing intercept"))?;
    let threshold = map
        .remove("threshold")
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "missing threshold"))?;

    let mut coeffs = [0.0f64; FEATURE_COUNT];
    for (i, name) in FEATURE_NAMES.iter().enumerate() {
        coeffs[i] = map.remove(*name).unwrap_or(0.0);
    }

    // Warn about unrecognised keys (helps catch typos / stale feature names).
    if !map.is_empty() {
        eprintln!(
            "criticality.coeffs: ignoring unrecognised keys: {:?}",
            map.keys().collect::<Vec<_>>()
        );
    }

    Ok(CriticalityCoeffs {
        intercept,
        threshold,
        coeffs,
    })
}

// ── LMR reduction ────────────────────────────────────────────────────────────

pub(in crate::search) fn compute_lmr_reduction_details(
    input: LmrInput,
    ctx: &mut SearchContext,
) -> LmrReduction {
    if input.moves_searched < LMR_FULL_DEPTH_MOVES
        || input.depth < LMR_MIN_DEPTH
        || input.is_capture
        || input.is_promo
        || input.gives_check
        || input.in_check
    {
        return LmrReduction {
            base_reduction: 0,
            final_reduction: 0,
        };
    }
    ctx.stats.lmr_attempts += 1;
    let move_count = input.moves_searched - LMR_FULL_DEPTH_MOVES + 1;
    let depth_ln = (input.depth as f64).ln();
    let move_ln = (move_count as f64).ln();
    let mut reduction = (0.5 + depth_ln * move_ln / LMR_LOG_DIVISOR).floor() as i32;
    let base_reduction = reduction;

    let history_bonus =
        input.history_score.max(0).clamp(0, LMR_HISTORY_CLAMP) / LMR_HISTORY_NORMALIZER;
    reduction -= history_bonus;

    if LMR_NODE_TYPE_SCALING {
        if input.is_pv {
            reduction = (reduction * 3 + 3) / 4;
        } else if input.is_cut_node {
            reduction = (reduction * 23 + 10) / 20;
        }
    }

    if input.improving {
        reduction += LMR_IMPROVING_BONUS;
    }

    let pre_protection_reduction = reduction.clamp(0, input.depth - 2);

    // Use the runtime-loaded threshold if available, otherwise the hardcoded
    // constant.
    let threshold = get_coefficients()
        .map(|c| c.threshold)
        .unwrap_or(CRITICALITY_P97_LOGIT);

    if pre_protection_reduction > 0
        && criticality_score(input, base_reduction, pre_protection_reduction) >= threshold
    {
        reduction -= 1;
    }

    LmrReduction {
        base_reduction,
        final_reduction: reduction.clamp(0, input.depth - 2),
    }
}

// ── Criticality score ────────────────────────────────────────────────────────

pub(in crate::search) fn criticality_score(
    input: LmrInput,
    base_reduction: i32,
    final_reduction: i32,
) -> f64 {
    if let Some(coeffs) = get_coefficients() {
        compute_score_from_coeffs(input, base_reduction, final_reduction, coeffs)
    } else {
        legacy_criticality_score(input, base_reduction, final_reduction)
    }
}

/// Compute the criticality logit using the loaded coefficients.
/// Feature normalisation and order must exactly match `tools/train.py`.
fn compute_score_from_coeffs(
    input: LmrInput,
    base_reduction: i32,
    final_reduction: i32,
    coeffs: &CriticalityCoeffs,
) -> f64 {
    let new_depth = if final_reduction > 0 {
        (input.depth - 1 - final_reduction).max(1)
    } else {
        input.depth - 1
    };
    let prev_static_eval = input.prev_static_eval.unwrap_or(0);
    let static_eval_delta = input
        .prev_static_eval
        .map_or(0, |prev| input.static_eval - prev);
    let piece = if input.moving_piece == PIECE_NONE {
        PieceType::None
    } else {
        piece_type(input.moving_piece)
    };

    // Feature vector in canonical order — must match FEATURE_NAMES.
    let feat: [f64; FEATURE_COUNT] = [
        input.root_depth as f64 / 16.0,                             // root_depth
        input.ply as f64 / 32.0,                                    // ply
        input.depth as f64 / 16.0,                                  // depth
        input.move_index as f64 / 32.0,                             // move_index
        base_reduction as f64 / 4.0,                                // base_reduction
        final_reduction as f64 / 4.0,                               // final_reduction
        new_depth as f64 / 16.0,                                    // new_depth
        normalized_history(input.history_score),                    // history_score
        normalized_score(input.static_eval),                        // static_eval
        bool_feature(input.prev_static_eval.is_some()),             // has_prev_static_eval
        normalized_score(prev_static_eval),                         // prev_static_eval
        normalized_score(static_eval_delta),                        // static_eval_delta
        normalized_score(input.alpha),                              // alpha
        normalized_score(input.beta),                               // beta
        bool_feature(input.is_pv),                                  // is_pv
        bool_feature(input.is_cut_node),                            // is_cut_node
        bool_feature(input.improving),                              // improving
        bool_feature(input.is_killer),                              // is_killer
        bool_feature(input.is_counter),                             // is_counter
        bool_feature(input.tt_move_agreement),                      // tt_move_agreement
        bool_feature(input.side_to_move == Color::Black),           // side_to_move_black
        bool_feature(piece == PieceType::Pawn),                     // piece_pawn
        bool_feature(piece == PieceType::Knight),                   // piece_knight
        bool_feature(piece == PieceType::Bishop),                   // piece_bishop
        bool_feature(piece == PieceType::Rook),                     // piece_rook
        bool_feature(piece == PieceType::Queen),                    // piece_queen
        bool_feature(piece == PieceType::King),                     // piece_king
    ];

    let mut score = coeffs.intercept;
    for (&c, &f) in coeffs.coeffs.iter().zip(feat.iter()) {
        score += c * f;
    }
    score
}

// ── Legacy hardcoded criticality model (fallback) ────────────────────────────
//
// Trained on a 200-game shadow-only dataset.  Kept as the fallback when no
// criticality.coeffs file is present next to the executable.

fn legacy_criticality_score(
    input: LmrInput,
    base_reduction: i32,
    final_reduction: i32,
) -> f64 {
    let new_depth = if final_reduction > 0 {
        (input.depth - 1 - final_reduction).max(1)
    } else {
        input.depth - 1
    };
    let prev_static_eval = input.prev_static_eval.unwrap_or(0);
    let static_eval_delta = input
        .prev_static_eval
        .map_or(0, |prev| input.static_eval - prev);
    let piece = if input.moving_piece == PIECE_NONE {
        PieceType::None
    } else {
        piece_type(input.moving_piece)
    };

    CRITICALITY_INTERCEPT
        - 0.152_875_012_601_427_25 * (input.root_depth as f64 / 16.0)
        + 0.735_838_646_984_717_2 * (input.ply as f64 / 32.0)
        - 0.873_714_182_579_038_2 * (input.depth as f64 / 16.0)
        - 1.043_699_758_610_700_6 * (input.move_index as f64 / 32.0)
        - 0.513_712_172_623_182_1 * (base_reduction as f64 / 4.0)
        + 1.544_485_099_792_215 * (final_reduction as f64 / 4.0)
        - 2.159_049_968_790_647_3 * (new_depth as f64 / 16.0)
        + 1.556_016_650_597_994_2 * normalized_history(input.history_score)
        + 5.739_198_384_264_096 * normalized_score(input.static_eval)
        - 0.316_867_213_444_681_1 * bool_feature(input.prev_static_eval.is_some())
        + 1.901_766_259_089_970_9 * normalized_score(prev_static_eval)
        + 4.004_276_897_688_178 * normalized_score(static_eval_delta)
        - 4.994_272_043_215_292 * normalized_score(input.alpha)
        - 0.382_227_234_880_739_06 * normalized_score(input.beta)
        + 0.584_084_076_062_692_6 * bool_feature(input.is_pv)
        - 0.584_084_076_062_692_7 * bool_feature(input.is_cut_node)
        + 0.447_596_124_223_620_94 * bool_feature(input.improving)
        - 1.947_485_685_342_310_6 * bool_feature(input.is_counter)
        - 0.011_544_318_219_468_804 * bool_feature(input.side_to_move == Color::Black)
        + 0.295_647_661_544_235_83 * bool_feature(piece == PieceType::Pawn)
        - 0.108_058_810_201_738_88 * bool_feature(piece == PieceType::Knight)
        - 0.059_180_402_269_422_84 * bool_feature(piece == PieceType::Bishop)
        - 0.123_707_657_637_899_55 * bool_feature(piece == PieceType::Rook)
        - 0.295_075_323_807_634_35 * bool_feature(piece == PieceType::Queen)
        + 0.130_156_508_673_690_16 * bool_feature(piece == PieceType::King)
        // is_killer and tt_move_agreement trained to exactly zero; omitted.
}

// ── Shared normalisation helpers ─────────────────────────────────────────────

pub(in crate::search) fn normalized_score(score: Score) -> f64 {
    score.clamp(-2_000, 2_000) as f64 / 2_000.0
}

pub(in crate::search) fn normalized_history(history_score: i32) -> f64 {
    history_score.clamp(-16_384, 16_384) as f64 / 16_384.0
}

pub(in crate::search) fn bool_feature(value: bool) -> f64 {
    if value {
        1.0
    } else {
        0.0
    }
}
