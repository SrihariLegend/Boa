#!/usr/bin/env python3
"""
Texel Tuner for Karpov chess engine.

Uses the Texel tuning method to optimize evaluation parameters by minimizing
the mean squared error between predicted game outcomes (via sigmoid of eval)
and actual game results from master games.

The tuner:
1. Loads quiet positions + results from training_data.txt
2. Calls the Karpov engine to evaluate each position
3. Optimizes eval constants via local search to minimize MSE

Training data format (one per line):
    <fen> <result>   where result is 1.0, 0.5, or 0.0

Usage:
    python3 tools/texel_tuner.py [--positions N] [--epochs N] [--k K]

References:
    - https://www.chessprogramming.org/Texel%27s_Tuning_Method
    - Peter Österlund's original description
"""

import subprocess
import os
import sys
import time
import math
import random
import argparse
import re
from concurrent.futures import ThreadPoolExecutor, as_completed

# ============================================================
# Configuration
# ============================================================

DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVAL_RS = os.path.join(DIR, "src", "eval.rs")
TRAINING_DATA = os.path.join(DIR, "tools", "training_data.txt")
BIN = os.path.join(DIR, "target", "release", "karpov")

GREEN  = "\033[92m"
CYAN   = "\033[96m"
RED    = "\033[91m"
YELLOW = "\033[93m"
BOLD   = "\033[1m"
DIM    = "\033[2m"
RESET  = "\033[0m"


# ============================================================
# Tunable parameters — extracted from eval.rs
# ============================================================
# Each entry: (name, rust_const_name, type, current_value, min, max, step)
#   type: "i32" for single ints, "(i32,i32)" for mg/eg tuples,
#         "[i32;N]" for arrays, "[(i32,i32);N]" for tuple arrays

TUNABLE_PARAMS = [
    # --- Piece activity ---
    ("bishop_pair_mg",          "BISHOP_PAIR_BONUS",           "mg",   25,  -10,  80,  5),
    ("bishop_pair_eg",          "BISHOP_PAIR_BONUS",           "eg",   80,    0, 120,  5),
    ("rook_open_file_mg",       "ROOK_OPEN_FILE_BONUS",        "mg",   25,   -5,  50,  5),
    ("rook_open_file_eg",       "ROOK_OPEN_FILE_BONUS",        "eg",    5,  -10,  35,  5),
    ("rook_semi_open_mg",       "ROOK_SEMI_OPEN_FILE_BONUS",   "mg",   19,   -5,  35,  3),
    ("rook_semi_open_eg",       "ROOK_SEMI_OPEN_FILE_BONUS",   "eg",    8,   -5,  25,  3),
    ("rook_seventh_mg",         "ROOK_ON_SEVENTH_BONUS",       "mg",   50,    0,  75,  5),
    ("rook_seventh_eg",         "ROOK_ON_SEVENTH_BONUS",       "eg",   35,    0,  65,  5),
    ("outpost_supported",       "OUTPOST_SUPPORTED",           "i32",   5,   -5,  50,  5),
    ("outpost_unsupported",     "OUTPOST_UNSUPPORTED",         "i32",  10,   -5,  35,  5),
    ("tempo",                   "TEMPO_BONUS",                 "i32",  24,    5,  50,  3),

    # --- Pawn structure ---
    ("doubled_mg",              "DOUBLED_PAWN_PENALTY",        "mg",  -13,  -35,   5,  3),
    ("doubled_eg",              "DOUBLED_PAWN_PENALTY",        "eg",    0,  -40,  10,  5),
    ("isolated_mg",             "ISOLATED_PAWN_PENALTY",       "mg",  -15,  -40,   5,  5),
    ("isolated_eg",             "ISOLATED_PAWN_PENALTY",       "eg",  -40,  -65,   0,  5),
    ("backward_mg",             "BACKWARD_PAWN_PENALTY",       "mg",  -23,  -40,   0,  3),
    ("backward_eg",             "BACKWARD_PAWN_PENALTY",       "eg",  -30,  -50,   0,  3),
    ("chain_mg",                "PAWN_CHAIN_BONUS",            "mg",    6,    0,  15,  1),
    ("chain_eg",                "PAWN_CHAIN_BONUS",            "eg",   15,    0,  25,  2),

    # --- Passed pawns ---
    ("passed_mg_r2",            "PASSED_PAWN_BONUS_MG[1]",     "arr",   2,   -5,  25,  3),
    ("passed_mg_r3",            "PASSED_PAWN_BONUS_MG[2]",     "arr",   0,   -5,  35,  5),
    ("passed_mg_r4",            "PASSED_PAWN_BONUS_MG[3]",     "arr",   5,   -5,  50,  5),
    ("passed_mg_r5",            "PASSED_PAWN_BONUS_MG[4]",     "arr",  15,   -5,  75,  5),
    ("passed_mg_r6",            "PASSED_PAWN_BONUS_MG[5]",     "arr",  75,   20, 120, 10),
    ("passed_mg_r7",            "PASSED_PAWN_BONUS_MG[6]",     "arr",  70,   20, 160, 10),
    ("passed_eg_r2",            "PASSED_PAWN_BONUS_EG[1]",     "arr",  10,   -5,  35,  5),
    ("passed_eg_r3",            "PASSED_PAWN_BONUS_EG[2]",     "arr",   5,   -5,  50,  5),
    ("passed_eg_r4",            "PASSED_PAWN_BONUS_EG[3]",     "arr",  20,   -5,  80, 10),
    ("passed_eg_r5",            "PASSED_PAWN_BONUS_EG[4]",     "arr",  65,   10, 130, 10),
    ("passed_eg_r6",            "PASSED_PAWN_BONUS_EG[5]",     "arr",  95,   30, 180, 15),
    ("passed_eg_r7",            "PASSED_PAWN_BONUS_EG[6]",     "arr", 115,   40, 260, 15),

    # --- King safety ---
    ("shield_per_pawn",         "PAWN_SHIELD_PER_PAWN",        "i32",  22,    3,  35,  2),
    ("shield_base_penalty",     "PAWN_SHIELD_BASE_PENALTY",    "i32",  30,   10,  60,  5),
    ("king_atk_knight",         "KING_ATTACK_WEIGHT_KNIGHT",   "i32",   2,    1,   8,  1),
    ("king_atk_bishop",         "KING_ATTACK_WEIGHT_BISHOP",   "i32",   3,    1,   8,  1),
    ("king_atk_rook",           "KING_ATTACK_WEIGHT_ROOK",     "i32",   1,    1,  10,  1),
    ("king_atk_queen",          "KING_ATTACK_WEIGHT_QUEEN",    "i32",   5,    2,  12,  1),

    # --- Freedom / squeeze ---
    ("squeeze_lockdown",        "SQUEEZE_TOTAL_LOCKDOWN",      "i32",  80,   20, 160, 10),
    ("squeeze_severe_base",     "SQUEEZE_SEVERE_BASE",         "i32",  20,    0, 100, 10),
    ("squeeze_severe_per",      "SQUEEZE_SEVERE_PER_MOVE",     "i32",   4,    0,  12,  1),
    ("squeeze_moderate_base",   "SQUEEZE_MODERATE_BASE",       "i32",  10,    0,  60,  5),
    ("squeeze_moderate_per",    "SQUEEZE_MODERATE_PER_MOVE",   "i32",   1,    0,  10,  1),

    # --- Trade-down ---
    ("trade_down_per_100",      "TRADE_DOWN_BONUS_PER_100CP",  "i32",  15,    1,  25,  2),

    # --- Weak squares ---
    ("weak_sq_control_mg",      "WEAK_SQUARE_CONTROL_BONUS",   "mg",    1,   -5,  20,  2),
    ("weak_sq_control_eg",      "WEAK_SQUARE_CONTROL_BONUS",   "eg",    1,   -5,  15,  2),
    ("weak_sq_knight_mg",       "WEAK_SQUARE_KNIGHT_BONUS",    "mg",   20,    0,  45,  5),
    ("weak_sq_knight_eg",       "WEAK_SQUARE_KNIGHT_BONUS",    "eg",    9,   -5,  35,  3),

    # --- Piece coordination ---
    ("coord_bonus_mg",          "PIECE_COORDINATION_BONUS",    "mg",    5,   -3,  12,  1),
    ("coord_bonus_eg",          "PIECE_COORDINATION_BONUS",    "eg",    0,   -5,  12,  1),
    ("central_overlap_mg",      "CENTRAL_CONTROL_OVERLAP_BONUS","mg",   1,   -3,  12,  1),
    ("central_overlap_eg",      "CENTRAL_CONTROL_OVERLAP_BONUS","eg",   8,   -3,  15,  1),

    # --- Data-driven terms ---
    ("center_piece_mg",         "PIECE_IN_CENTER_BONUS",       "mg",    0,   -5,  20,  2),
    ("center_piece_eg",         "PIECE_IN_CENTER_BONUS",       "eg",    9,   -5,  20,  2),
    ("far_king_pen_mg",         "PIECE_FAR_FROM_KING_PENALTY", "mg",   -2,  -15,   3,  1),
    ("far_king_pen_eg",         "PIECE_FAR_FROM_KING_PENALTY", "eg",    0,  -10,   3,  1),
    ("quadrant_spread_mg",      "QUADRANT_SPREAD_BONUS",       "mg",    9,   -5,  20,  2),
    ("quadrant_spread_eg",      "QUADRANT_SPREAD_BONUS",       "eg",   11,   -5,  25,  2),
    ("ext_center_atk_mg",       "EXTENDED_CENTER_ATTACK_BONUS","mg",    0,   -3,   8,  1),
    ("ext_center_atk_eg",       "EXTENDED_CENTER_ATTACK_BONUS","eg",    5,   -3,  10,  1),
    ("adv_pawn_r5_mg",          "ADVANCED_PAWN_BONUS_MG[0]",   "arr",   7,   -5,  20,  2),
    ("adv_pawn_r6_mg",          "ADVANCED_PAWN_BONUS_MG[1]",   "arr",  15,   -5,  35,  3),
    ("adv_pawn_r7_mg",          "ADVANCED_PAWN_BONUS_MG[2]",   "arr",  30,    0,  55,  5),
    ("adv_pawn_r5_eg",          "ADVANCED_PAWN_BONUS_EG[0]",   "arr",   2,   -5,  25,  3),
    ("adv_pawn_r6_eg",          "ADVANCED_PAWN_BONUS_EG[1]",   "arr",   8,   -5,  40,  5),
    ("adv_pawn_r7_eg",          "ADVANCED_PAWN_BONUS_EG[2]",   "arr",  35,    0,  75,  5),

    # --- Passed pawn support ---
    ("passer_path_clear_mg",    "PASSER_PATH_CLEAR_BONUS",     "mg",    2,   -5,  25,  3),
    ("passer_path_clear_eg",    "PASSER_PATH_CLEAR_BONUS",     "eg",   15,   -5,  45,  5),
    ("passer_king_prox_eg",     "PASSER_KING_PROXIMITY_EG",    "i32",  15,    0,  25,  2),
    ("passer_enemy_dist_eg",    "PASSER_ENEMY_KING_DIST_EG",   "i32",  10,    0,  18,  1),
    ("rook_behind_passer_mg",   "ROOK_BEHIND_PASSER_BONUS",    "mg",    5,   -5,  40,  5),
    ("rook_behind_passer_eg",   "ROOK_BEHIND_PASSER_BONUS",    "eg",   10,   -5,  50,  5),
    ("connected_passer_mg",     "CONNECTED_PASSER_BONUS",      "mg",    0,   -5,  35,  5),
    ("connected_passer_eg",     "CONNECTED_PASSER_BONUS",      "eg",    5,   -5,  45,  5),
    ("king_central_eg",         "KING_CENTRALIZATION_EG",      "i32",  15,    0,  25,  2),
]


# ============================================================
# Training data loading
# ============================================================

def load_training_data(path, max_positions=0):
    """Load FEN + result pairs from training data file."""
    positions = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            # Format: "fen_fields result"
            # FEN has 6 fields, result is the 7th token
            parts = line.rsplit(' ', 1)
            if len(parts) != 2:
                continue
            fen = parts[0]
            try:
                result = float(parts[1])
            except ValueError:
                continue
            positions.append((fen, result))
            if max_positions > 0 and len(positions) >= max_positions:
                break
    return positions


# ============================================================
# Engine evaluation via UCI
# ============================================================

def _eval_single_batch(binary, batch):
    """Evaluate a single batch of FENs using one engine process."""
    proc = subprocess.Popen(
        [binary],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1
    )
    
    commands = ["uci\n", "isready\n"]
    for fen in batch:
        commands.append(f"position fen {fen}\n")
        commands.append("eval\n")
    commands.append("quit\n")
    
    try:
        stdout, _ = proc.communicate("".join(commands), timeout=120)
    except subprocess.TimeoutExpired:
        proc.kill()
        stdout, _ = proc.communicate()
    
    scores = []
    for line in stdout.split('\n'):
        if line.startswith("eval:"):
            m = re.search(r'eval:\s*(-?\d+)\s*cp', line)
            if m:
                scores.append(int(m.group(1)))
    return scores


def eval_positions_batch(binary, fens, batch_size=500):
    """Evaluate positions by sending them to the engine via UCI.
    
    Uses multiple engine processes in parallel via ThreadPoolExecutor.
    Returns list of scores in centipawns (from white's perspective).
    """
    batches = []
    for batch_start in range(0, len(fens), batch_size):
        batches.append(fens[batch_start:batch_start + batch_size])
    
    # Use up to 4 parallel engine processes (engine itself is single-threaded)
    max_workers = min(4, len(batches))
    all_scores = [None] * len(batches)
    
    with ThreadPoolExecutor(max_workers=max_workers) as executor:
        futures = {
            executor.submit(_eval_single_batch, binary, batch): idx
            for idx, batch in enumerate(batches)
        }
        for future in as_completed(futures):
            idx = futures[future]
            all_scores[idx] = future.result()
    
    # Flatten in order
    scores = []
    for batch_scores in all_scores:
        scores.extend(batch_scores)
    
    return scores


def eval_positions_white_pov(binary, positions, batch_size=500):
    """Evaluate all positions and return scores from WHITE's perspective."""
    fens = [p[0] for p in positions]
    raw_scores = eval_positions_batch(binary, fens, batch_size)
    
    if len(raw_scores) != len(fens):
        print(f"  {RED}WARNING: Got {len(raw_scores)} scores for {len(fens)} positions{RESET}")
        return None
    
    # Convert to white's perspective
    # Engine returns score from side-to-move perspective
    white_scores = []
    for i, (fen, _) in enumerate(positions):
        score = raw_scores[i]
        # Determine side to move from FEN
        parts = fen.split()
        if len(parts) >= 2 and parts[1] == 'b':
            score = -score  # Flip for black to move
        white_scores.append(score)
    
    return white_scores


# ============================================================
# Texel tuning: sigmoid and MSE
# ============================================================

def sigmoid(score, k):
    """Convert centipawn score to win probability using sigmoid.
    
    k controls the steepness. Typical values: 1.0 - 1.5
    score is in centipawns from white's perspective.
    """
    # Clamp to avoid overflow
    x = k * score / 400.0
    if x > 20:
        return 1.0
    if x < -20:
        return 0.0
    return 1.0 / (1.0 + math.exp(-x))


def compute_mse(scores, results, k):
    """Compute mean squared error between sigmoid(score) and actual result."""
    total = 0.0
    for score, result in zip(scores, results):
        predicted = sigmoid(score, k)
        total += (result - predicted) ** 2
    return total / len(scores)


def find_optimal_k(scores, results):
    """Find the sigmoid scaling factor K that minimizes MSE.
    
    Uses golden section search on [0.5, 3.0].
    """
    a, b = 0.5, 3.0
    gr = (math.sqrt(5) + 1) / 2
    tol = 0.001
    
    c = b - (b - a) / gr
    d = a + (b - a) / gr
    
    while abs(b - a) > tol:
        fc = compute_mse(scores, results, c)
        fd = compute_mse(scores, results, d)
        if fc < fd:
            b = d
        else:
            a = c
        c = b - (b - a) / gr
        d = a + (b - a) / gr
    
    return (a + b) / 2


# ============================================================
# Source code modification
# ============================================================

def read_eval_source():
    """Read eval.rs source code."""
    with open(EVAL_RS) as f:
        return f.read()


def write_eval_source(src):
    """Write eval.rs source code."""
    with open(EVAL_RS, 'w') as f:
        f.write(src)


def apply_param_to_source(src, param_name, rust_const, ptype, value):
    """Apply a single parameter value to the source code.
    
    Handles different constant formats:
    - "i32": const FOO: i32 = VALUE;
    - "mg": first element of tuple const FOO: (i32, i32) = (MG, EG);
    - "eg": second element of tuple const FOO: (i32, i32) = (MG, EG);
    - "arr": array element like PASSED_PAWN_BONUS_MG[2]
    """
    value = int(value)
    
    if ptype == "i32":
        # Simple integer constant
        pattern = rf'(const\s+{re.escape(rust_const)}\s*:\s*i32\s*=\s*)(-?\d+)(\s*;)'
        replacement = rf'\g<1>{value}\3'
        new_src = re.sub(pattern, replacement, src)
        return new_src
    
    if ptype in ("mg", "eg"):
        # Tuple constant: (mg, eg)
        pattern = rf'(const\s+{re.escape(rust_const)}\s*:\s*\(i32,\s*i32\)\s*=\s*\()(-?\d+)\s*,\s*(-?\d+)(\)\s*;)'
        m = re.search(pattern, src)
        if not m:
            return src
        old_mg = int(m.group(2))
        old_eg = int(m.group(3))
        if ptype == "mg":
            new_mg, new_eg = value, old_eg
        else:
            new_mg, new_eg = old_mg, value
        replacement = f'{m.group(1)}{new_mg}, {new_eg}{m.group(4)}'
        return src[:m.start()] + replacement + src[m.end():]
    
    if ptype == "arr":
        # Array element like PASSED_PAWN_BONUS_MG[2]
        # Extract array name and index
        arr_match = re.match(r'(.+)\[(\d+)\]', rust_const)
        if not arr_match:
            return src
        arr_name = arr_match.group(1)
        arr_idx = int(arr_match.group(2))
        
        # Find the const declaration
        pattern = rf'(const\s+{re.escape(arr_name)}\s*:\s*\[i32;\s*\d+\]\s*=\s*\[)([^\]]+)(\]\s*;)'
        m = re.search(pattern, src)
        if not m:
            return src
        
        elements = [x.strip() for x in m.group(2).split(',')]
        if arr_idx < len(elements):
            elements[arr_idx] = str(value)
        new_arr = ', '.join(elements)
        replacement = f'{m.group(1)}{new_arr}{m.group(3)}'
        return src[:m.start()] + replacement + src[m.end():]
    
    return src


def apply_params_to_source(src, params, values):
    """Apply all parameter values to source code."""
    for i, (name, rust_const, ptype, _default, _mn, _mx, _step) in enumerate(params):
        src = apply_param_to_source(src, name, rust_const, ptype, values[i])
    return src


def build_engine():
    """Build the engine in release mode. Returns True on success."""
    result = subprocess.run(
        ["cargo", "build", "--release", "-j", "24"],
        cwd=DIR,
        capture_output=True,
        text=True
    )
    return result.returncode == 0


# ============================================================
# Texel tuning loop
# ============================================================

def texel_tune(positions, params, initial_values, k, epochs=100, patience=5):
    """
    Local search Texel tuning.
    
    For each parameter, try +step and -step. If MSE improves, keep the change.
    Repeat until no improvement for `patience` epochs.
    """
    original_src = read_eval_source()
    values = list(initial_values)
    results = [p[1] for p in positions]
    
    # Get baseline MSE
    print(f"\n  {BOLD}Building baseline...{RESET}")
    if not build_engine():
        print(f"  {RED}Build failed!{RESET}")
        return values
    
    scores = eval_positions_white_pov(BIN, positions)
    if scores is None:
        print(f"  {RED}Evaluation failed!{RESET}")
        return values
    
    best_mse = compute_mse(scores, results, k)
    print(f"  {BOLD}Baseline MSE: {best_mse:.8f}{RESET}")
    print(f"  {DIM}K = {k:.4f}, {len(positions)} positions, {len(params)} parameters{RESET}\n")
    
    no_improve_epochs = 0
    total_improvements = 0
    
    for epoch in range(epochs):
        epoch_improved = False
        improved_this_epoch = 0
        
        print(f"  {BOLD}{CYAN}═══ Epoch {epoch + 1}/{epochs} ═══{RESET}  (best MSE: {best_mse:.8f})")
        
        # Shuffle parameter order each epoch for better exploration
        param_order = list(range(len(params)))
        random.shuffle(param_order)
        
        for pi in param_order:
            name, rust_const, ptype, _default, mn, mx, step = params[pi]
            old_val = values[pi]
            
            improved = False
            
            for delta in [step, -step]:
                new_val = old_val + delta
                if new_val < mn or new_val > mx:
                    continue
                
                values[pi] = new_val
                
                # Apply to source and rebuild
                src = apply_params_to_source(original_src, params, values)
                write_eval_source(src)
                
                if not build_engine():
                    values[pi] = old_val
                    continue
                
                new_scores = eval_positions_white_pov(BIN, positions)
                if new_scores is None:
                    values[pi] = old_val
                    continue
                
                new_mse = compute_mse(new_scores, results, k)
                
                if new_mse < best_mse:
                    improvement = best_mse - new_mse
                    best_mse = new_mse
                    scores = new_scores
                    old_val = new_val
                    improved = True
                    epoch_improved = True
                    improved_this_epoch += 1
                    total_improvements += 1
                    print(f"    {GREEN}✓ {name}: {old_val - delta} → {new_val}  "
                          f"(MSE: {new_mse:.8f}, Δ: -{improvement:.8f}){RESET}")
                    break
                else:
                    values[pi] = old_val
            
            if not improved:
                values[pi] = old_val
        
        print(f"  {DIM}  Epoch {epoch + 1} done: {improved_this_epoch} improvements, MSE = {best_mse:.8f}{RESET}\n")
        
        if epoch_improved:
            no_improve_epochs = 0
        else:
            no_improve_epochs += 1
            if no_improve_epochs >= patience:
                print(f"  {YELLOW}No improvement for {patience} epochs — converged.{RESET}")
                break
    
    # Write final values
    final_src = apply_params_to_source(original_src, params, values)
    write_eval_source(final_src)
    build_engine()
    
    print(f"\n  {BOLD}{GREEN}Total improvements: {total_improvements}{RESET}")
    print(f"  {BOLD}Final MSE: {best_mse:.8f}{RESET}")
    
    return values


# ============================================================
# Main
# ============================================================

def main():
    parser = argparse.ArgumentParser(description="Texel tuner for Karpov chess engine")
    parser.add_argument("--positions", type=int, default=50000,
                        help="Max positions to use (default 50000)")
    parser.add_argument("--epochs", type=int, default=100,
                        help="Max tuning epochs (default 100)")
    parser.add_argument("--patience", type=int, default=3,
                        help="Stop after N epochs without improvement (default 3)")
    parser.add_argument("--k", type=float, default=0,
                        help="Sigmoid K factor (0 = auto-detect)")
    parser.add_argument("--batch-size", type=int, default=2000,
                        help="Engine eval batch size (default 2000)")
    parser.add_argument("--dry-run", action="store_true",
                        help="Just compute baseline MSE, don't tune")
    args = parser.parse_args()
    
    print(f"\n{BOLD}{CYAN}  ╔══════════════════════════════════════╗{RESET}")
    print(f"{BOLD}{CYAN}  ║   Karpov Texel Tuner                 ║{RESET}")
    print(f"{BOLD}{CYAN}  ╚══════════════════════════════════════╝{RESET}\n")
    
    # Check prerequisites
    if not os.path.exists(TRAINING_DATA):
        print(f"  {RED}Training data not found: {TRAINING_DATA}{RESET}")
        print(f"  Run: python3 tools/extract_training_data.py")
        sys.exit(1)
    
    if not os.path.exists(BIN):
        print(f"  {YELLOW}Engine not built. Building...{RESET}")
        if not build_engine():
            print(f"  {RED}Build failed!{RESET}")
            sys.exit(1)
    
    # Load training data
    print(f"  Loading training data...")
    positions = load_training_data(TRAINING_DATA, args.positions)
    print(f"  Loaded {len(positions)} positions")
    
    results = [p[1] for p in positions]
    wins = sum(1 for r in results if r == 1.0)
    draws = sum(1 for r in results if r == 0.5)
    losses = sum(1 for r in results if r == 0.0)
    print(f"  Distribution: {wins} white wins, {draws} draws, {losses} white losses")
    
    # Evaluate all positions with current parameters
    print(f"\n  Evaluating positions with current engine...")
    t0 = time.time()
    scores = eval_positions_white_pov(BIN, positions, args.batch_size)
    elapsed = time.time() - t0
    
    if scores is None or len(scores) != len(positions):
        print(f"  {RED}Failed to evaluate positions! Got {len(scores) if scores else 0}/{len(positions)}{RESET}")
        sys.exit(1)
    
    print(f"  Evaluated {len(scores)} positions in {elapsed:.1f}s ({len(scores)/elapsed:.0f} pos/s)")
    
    # Find or use K
    if args.k > 0:
        k = args.k
        mse = compute_mse(scores, results, k)
        print(f"\n  Using K = {k:.4f} (user-specified)")
    else:
        print(f"\n  Finding optimal K...")
        k = find_optimal_k(scores, results)
        mse = compute_mse(scores, results, k)
        print(f"  Optimal K = {k:.4f}")
    
    print(f"  Baseline MSE = {mse:.8f}")
    
    # Show score distribution
    abs_scores = [abs(s) for s in scores]
    avg_abs = sum(abs_scores) / len(abs_scores)
    print(f"  Avg |eval| = {avg_abs:.1f} cp")
    
    if args.dry_run:
        print(f"\n  {YELLOW}Dry run — not tuning.{RESET}")
        print_current_values()
        return
    
    # Run tuning
    print(f"\n  {BOLD}Starting Texel tuning...{RESET}")
    print(f"  Parameters: {len(TUNABLE_PARAMS)}")
    print(f"  Epochs: {args.epochs}")
    print(f"  Patience: {args.patience}")
    
    initial_values = [p[3] for p in TUNABLE_PARAMS]  # current defaults
    
    final_values = texel_tune(
        positions,
        TUNABLE_PARAMS,
        initial_values,
        k,
        epochs=args.epochs,
        patience=args.patience,
    )
    
    # Print results
    print(f"\n  {BOLD}{CYAN}╔══════════════════════════════════════╗{RESET}")
    print(f"  {BOLD}{CYAN}║   Tuning Results                     ║{RESET}")
    print(f"  {BOLD}{CYAN}╚══════════════════════════════════════╝{RESET}\n")
    
    changes = 0
    for i, (name, rust_const, ptype, default, mn, mx, step) in enumerate(TUNABLE_PARAMS):
        old = default
        new = int(final_values[i])
        if old != new:
            changes += 1
            print(f"  {GREEN}  {name:30s}: {old:4d} → {new:4d}  ({'+' if new > old else ''}{new - old}){RESET}")
        else:
            print(f"  {DIM}  {name:30s}: {old:4d} (unchanged){RESET}")
    
    print(f"\n  {BOLD}{changes} parameters changed out of {len(TUNABLE_PARAMS)}{RESET}")
    print(f"  {BOLD}Changes written to {EVAL_RS}{RESET}")
    print(f"  {DIM}Don't forget to rebuild: cargo build --release -j 24{RESET}\n")


def print_current_values():
    """Print all tunable parameters and their current values."""
    print(f"\n  {BOLD}Current tunable parameters:{RESET}")
    for name, rust_const, ptype, default, mn, mx, step in TUNABLE_PARAMS:
        print(f"    {name:30s} = {default:4d}  (range: [{mn}, {mx}], step: {step})")


if __name__ == "__main__":
    main()
