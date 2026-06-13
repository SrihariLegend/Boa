#!/usr/bin/env python3
"""
Self-play ablation testing for Karpov chess engine.

Plays the baseline engine against a variant (with one eval term disabled)
in a series of games. Measures win/loss/draw to estimate Elo impact.

Usage:
    python3 tools/selfplay.py                    # Run all ablation tests
    python3 tools/selfplay.py freedom_metric     # Test single term
    python3 tools/selfplay.py --games 50         # Set number of games
"""

import subprocess
import os
import re
import sys
import shutil
import time
import math
import chess
import chess.engine
import asyncio
import random

ENGINE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVAL_RS = os.path.join(ENGINE_DIR, "src", "eval.rs")
SEARCH_RS = os.path.join(ENGINE_DIR, "src", "search.rs")
ENGINE_BIN = os.path.join(ENGINE_DIR, "target", "release", "karpov")
VARIANT_BIN = os.path.join(ENGINE_DIR, "target", "release", "karpov_variant")

GAMES_PER_TEST = 40  # 40 games per variant (20 as white, 20 as black)
TIME_PER_MOVE_MS = 200  # 200ms per move
DEPTH_LIMIT = 10
MAX_MOVES = 200  # Max half-moves per game

# Eval terms to test
EVAL_TERMS = [
    ("freedom_metric",        "freedom_metric",         "0"),
    ("prophylaxis_eval",      "prophylaxis_eval",       "(0, 0)"),
    ("piece_coordination",    "piece_coordination_eval","(0, 0)"),
    ("weak_square_eval",      "weak_square_eval",       "(0, 0)"),
    ("knight_vs_bishop",      "knight_vs_bishop_eval",  "(0, 0)"),
    ("bad_bishop_eval",       "bad_bishop_eval",        "(0, 0)"),
    ("space_evaluation",      "space_evaluation",       "(0, 0)"),
    ("trade_down_bonus",      "trade_down_bonus",       "(0, 0)"),
    ("king_safety",           "king_safety",            "(0, 0)"),
    ("mobility_and_activity", "mobility_and_activity",  "(0, 0)"),
    ("pawn_structure",        "pawn_structure",         "(0, 0)"),
]

# Opening book (diverse openings to reduce noise)
OPENINGS = [
    "d2d4 d7d5",                          # QGD
    "e2e4 e7e5",                          # Open game
    "d2d4 g8f6 c2c4 e7e6",              # Nimzo setup
    "e2e4 c7c5",                          # Sicilian
    "d2d4 d7d5 c2c4 e7e6",              # QGD classical
    "g1f3 d7d5 g2g3",                    # Reti
    "e2e4 e7e6",                          # French
    "d2d4 g8f6 c2c4 g7g6",              # KID setup
    "e2e4 c7c6",                          # Caro-Kann
    "d2d4 d7d5 g1f3 g8f6 c2c4 c7c6",   # Slav
]


def run_command(cmd, timeout=60):
    try:
        result = subprocess.run(
            cmd, shell=True, capture_output=True, text=True, timeout=timeout,
            cwd=ENGINE_DIR
        )
        return result.stdout + result.stderr, result.returncode
    except subprocess.TimeoutExpired:
        return "TIMEOUT", 1


def build_engine(binary_name="karpov"):
    """Build engine. Returns True on success."""
    out, rc = run_command(f"cargo build --release -j 24 2>&1", timeout=30)
    if "Finished" not in out and "Compiling" not in out:
        return False
    # Copy to variant binary if needed
    src = os.path.join(ENGINE_DIR, "target", "release", "karpov")
    dst = os.path.join(ENGINE_DIR, "target", "release", binary_name)
    if binary_name != "karpov":
        shutil.copy2(src, dst)
    return True


def disable_eval_term(original_code, func_name, zero_value):
    """Insert early return to disable a function."""
    pattern = rf'(fn {func_name}\([^)]*\)[^{{]*\{{)'
    match = re.search(pattern, original_code)
    if not match:
        return None
    insert_pos = match.end()
    modified = original_code[:insert_pos] + f'\n    return {zero_value}; // ABLATION\n' + original_code[insert_pos:]
    return modified


async def play_game(engine_w_path, engine_b_path, opening_moves_uci):
    """Play a single game. Returns ('w', 'b', or 'd') for white win, black win, draw."""
    transport_w, engine_w = await chess.engine.popen_uci(engine_w_path)
    transport_b, engine_b = await chess.engine.popen_uci(engine_b_path)

    board = chess.Board()

    # Play opening moves
    for move_uci in opening_moves_uci.split():
        move = chess.Move.from_uci(move_uci)
        if move in board.legal_moves:
            board.push(move)

    move_count = 0
    try:
        while not board.is_game_over() and move_count < MAX_MOVES:
            engine = engine_w if board.turn == chess.WHITE else engine_b
            limit = chess.engine.Limit(time=TIME_PER_MOVE_MS / 1000.0)
            result = await engine.play(board, limit)
            board.push(result.move)
            move_count += 1
    except Exception as e:
        print(f"  Game error: {e}")
    finally:
        await engine_w.quit()
        await engine_b.quit()

    if board.is_game_over():
        result = board.result()
        if result == "1-0":
            return 'w', move_count
        elif result == "0-1":
            return 'b', move_count
        else:
            return 'd', move_count
    else:
        return 'd', move_count  # Max moves reached = draw


async def run_match(engine_a_path, engine_b_path, num_games, label_a="Baseline", label_b="Variant"):
    """
    Run a match between two engines.
    Each opening is played twice (colors swapped).
    Returns (wins_a, wins_b, draws).
    """
    wins_a, wins_b, draws = 0, 0, 0
    games_played = 0

    # Use openings cyclically, each played from both sides
    for i in range(num_games // 2):
        opening = OPENINGS[i % len(OPENINGS)]

        for swap in [False, True]:
            if swap:
                w_path, b_path = engine_b_path, engine_a_path
            else:
                w_path, b_path = engine_a_path, engine_b_path

            result, moves = await play_game(w_path, b_path, opening)
            games_played += 1

            if result == 'w':
                if swap:
                    wins_b += 1
                    sym = f"{label_b}"
                else:
                    wins_a += 1
                    sym = f"{label_a}"
            elif result == 'b':
                if swap:
                    wins_a += 1
                    sym = f"{label_a}"
                else:
                    wins_b += 1
                    sym = f"{label_b}"
            else:
                draws += 1
                sym = "Draw"

            status = f"  Game {games_played}/{num_games}: {sym} wins ({moves} moves)" if result != 'd' else f"  Game {games_played}/{num_games}: Draw ({moves} moves)"
            print(status)

    return wins_a, wins_b, draws


def elo_diff(wins, losses, draws):
    """Estimate Elo difference from match results."""
    total = wins + losses + draws
    if total == 0:
        return 0, 0
    score = (wins + draws * 0.5) / total
    if score <= 0 or score >= 1:
        return 400 if score >= 1 else -400, 999
    elo = -400 * math.log10(1.0 / score - 1.0)
    # Standard error approximation
    se = math.sqrt(score * (1 - score) / total)
    elo_err = 400 * se / (score * (1 - score) * math.log(10)) if score > 0 and score < 1 else 999
    return elo, elo_err


async def main():
    num_games = GAMES_PER_TEST
    target_term = None

    for arg in sys.argv[1:]:
        if arg.startswith("--games"):
            continue
        elif sys.argv[sys.argv.index(arg) - 1] == "--games" if "--games" in sys.argv else False:
            num_games = int(arg)
        elif arg.isdigit():
            num_games = int(arg)
        else:
            target_term = arg

    # Handle --games N
    for i, arg in enumerate(sys.argv[1:], 1):
        if arg == "--games" and i < len(sys.argv) - 1:
            num_games = int(sys.argv[i + 1])

    print(f"Karpov Engine Self-Play Ablation")
    print(f"Games per test: {num_games}")
    print(f"Time per move: {TIME_PER_MOVE_MS}ms")
    print(f"=" * 60)

    # Read original source
    with open(EVAL_RS, 'r') as f:
        original_code = f.read()

    backup_path = EVAL_RS + ".bak"
    shutil.copy2(EVAL_RS, backup_path)

    try:
        # Build baseline
        print("\nBuilding baseline...")
        with open(EVAL_RS, 'w') as f:
            f.write(original_code)
        if not build_engine("karpov_baseline"):
            print("ERROR: Baseline build failed!")
            return
        baseline_bin = os.path.join(ENGINE_DIR, "target", "release", "karpov_baseline")

        terms_to_test = EVAL_TERMS
        if target_term:
            terms_to_test = [(n, fn, zv) for n, fn, zv in EVAL_TERMS if n == target_term]
            if not terms_to_test:
                print(f"Unknown term: {target_term}")
                print(f"Available: {', '.join(n for n, _, _ in EVAL_TERMS)}")
                return

        results = []
        for name, func_name, zero_val in terms_to_test:
            print(f"\n{'='*60}")
            print(f"Testing: {name}")
            print(f"{'='*60}")

            modified = disable_eval_term(original_code, func_name, zero_val)
            if modified is None:
                print(f"  Could not find function {func_name}")
                results.append((name, 0, 0, 0, 0, 0))
                continue

            with open(EVAL_RS, 'w') as f:
                f.write(modified)

            if not build_engine("karpov_variant"):
                print(f"  Build failed for {name}")
                results.append((name, 0, 0, 0, 0, 0))
                continue

            variant_bin = os.path.join(ENGINE_DIR, "target", "release", "karpov_variant")

            # Match: Baseline vs Variant (no term)
            wins_base, wins_var, draws = await run_match(
                baseline_bin, variant_bin, num_games,
                "Baseline", f"No-{name}"
            )

            elo, elo_err = elo_diff(wins_base, wins_var, draws)

            print(f"\n  Result: Baseline {wins_base} - {draws} - {wins_var} No-{name}")
            print(f"  Elo of term: {elo:+.0f} ±{elo_err:.0f}")

            results.append((name, wins_base, wins_var, draws, elo, elo_err))

        # Restore
        shutil.copy2(backup_path, EVAL_RS)
        build_engine()

        # Summary
        print(f"\n{'='*60}")
        print("SELF-PLAY ABLATION RESULTS")
        print(f"{'='*60}")
        print(f"{'Term':<25} {'W':>4} {'D':>4} {'L':>4} {'Elo':>8} {'±Err':>6}  Verdict")
        print("-" * 70)

        for name, w, l, d, elo, err in results:
            if w + l + d == 0:
                print(f"{name:<25} {'---':>4} {'---':>4} {'---':>4} {'---':>8} {'---':>6}  SKIP")
            else:
                if elo > err:
                    verdict = "HELPS ✓"
                elif elo < -err:
                    verdict = "HURTS ✗"
                else:
                    verdict = "UNCLEAR"
                print(f"{name:<25} {w:>4} {d:>4} {l:>4} {elo:>+7.0f} {err:>5.0f}  {verdict}")

        print()
        print("W/D/L from Baseline perspective (Baseline wins / Draws / Variant wins)")
        print("Positive Elo = term HELPS (baseline beats variant = having the term is better)")
        print("Negative Elo = term HURTS (variant beats baseline = not having it is better)")

    finally:
        if os.path.exists(backup_path):
            shutil.copy2(backup_path, EVAL_RS)
            os.remove(backup_path)
            build_engine()
        # Cleanup variant binaries
        for f in ["karpov_baseline", "karpov_variant"]:
            p = os.path.join(ENGINE_DIR, "target", "release", f)
            if os.path.exists(p):
                os.remove(p)


if __name__ == "__main__":
    asyncio.run(main())
