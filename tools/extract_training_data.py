#!/usr/bin/env python3
"""
Extract quiet positions from PGN games for Texel tuning.

Reads all PGN games from games/ directory and extracts FEN + result
for positions that are "quiet" (no captures, checks, or promotions
in the last move). Samples from moves 8-80 to avoid opening book
and extreme endgame positions.

Output: tools/training_data.txt
Format: <fen> <result>
  result: 1.0 (white win), 0.5 (draw), 0.0 (white loss)

Usage:
    python3 tools/extract_training_data.py [--max-games 0] [--sample-rate 2]
"""

import chess
import chess.pgn
import io
import os
import sys
import zipfile
import argparse
import time
import random
from concurrent.futures import ProcessPoolExecutor, as_completed

DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GAMES_DIR = os.path.join(DIR, "games")
OUTPUT = os.path.join(DIR, "tools", "training_data.txt")

GREEN  = "\033[92m"
CYAN   = "\033[96m"
BOLD   = "\033[1m"
DIM    = "\033[2m"
RESET  = "\033[0m"


def is_quiet(board, last_move):
    """Position is quiet: no check, last move wasn't a capture or promotion."""
    if board.is_check():
        return False
    if last_move is None:
        return False
    if board.is_capture(last_move):
        return False  # won't work after push, check before
    return True


def extract_from_game(game, sample_rate):
    """Extract quiet FEN positions from a game."""
    result_str = game.headers.get("Result", "*")
    if result_str == "1-0":
        result = "1.0"
    elif result_str == "0-1":
        result = "0.0"
    elif result_str == "1/2-1/2":
        result = "0.5"
    else:
        return []

    positions = []
    board = game.board()
    move_num = 0

    for node in game.mainline():
        move = node.move
        # Check capture BEFORE pushing
        is_cap = board.is_capture(move)
        is_promo = move.promotion is not None

        board.push(move)
        move_num += 1

        # Skip opening and deep endgame
        if move_num < 16 or move_num > 160:
            continue

        # Only sample every Nth move
        if move_num % sample_rate != 0:
            continue

        # Quiet: no captures, no promotions, no check
        if is_cap or is_promo:
            continue
        if board.is_check():
            continue

        # Skip positions with very few pieces (trivial endgames)
        if bin(board.occupied).count('1') < 6:
            continue

        fen = board.fen()
        positions.append(f"{fen} {result}")

    return positions


def load_games_from_zip(zip_path, max_games=0):
    """Load games from a zip file containing a PGN."""
    with zipfile.ZipFile(zip_path) as zf:
        pgn_name = [n for n in zf.namelist() if n.endswith('.pgn')][0]
        with zf.open(pgn_name) as f:
            pgn_text = f.read().decode('utf-8', errors='replace')

    pgn_io = io.StringIO(pgn_text)
    games = []
    while True:
        game = chess.pgn.read_game(pgn_io)
        if game is None:
            break
        games.append(game)
        if max_games > 0 and len(games) >= max_games:
            break
    return games


def main():
    parser = argparse.ArgumentParser(description="Extract Texel tuning data from PGN games")
    parser.add_argument("--max-games", type=int, default=0, help="Max games per player (0=all)")
    parser.add_argument("--sample-rate", type=int, default=2, help="Sample every Nth move (default 2)")
    args = parser.parse_args()

    zips = [
        ("Karpov", "Karpov.zip"),
        ("Petrosian", "Petrosian.zip"),
        ("Keres", "Keres.zip"),
    ]

    all_positions = []

    print(f"\n{BOLD}{CYAN}  Extracting quiet positions for Texel tuning...{RESET}\n")

    for player, zip_file in zips:
        zip_path = os.path.join(GAMES_DIR, zip_file)
        if not os.path.exists(zip_path):
            print(f"  ⚠ {zip_path} not found — skipping")
            continue

        print(f"  {BOLD}{player}{RESET}: loading...", end="", flush=True)
        games = load_games_from_zip(zip_path, args.max_games)
        print(f" {len(games)} games", end="", flush=True)

        t0 = time.time()
        player_positions = []
        # Process games in parallel using all available cores
        max_workers = min(24, len(games))
        with ProcessPoolExecutor(max_workers=max_workers) as executor:
            futures = [executor.submit(extract_from_game, game, args.sample_rate) for game in games]
            for i, future in enumerate(as_completed(futures)):
                player_positions.extend(future.result())
                if (i + 1) % 500 == 0:
                    print(f"\r  {BOLD}{player}{RESET}: {i+1}/{len(games)} games, {len(player_positions)} positions", end="", flush=True)

        elapsed = time.time() - t0
        print(f"\r  {BOLD}{player}{RESET}: {len(games)} games → {len(player_positions)} positions ({elapsed:.1f}s)")
        all_positions.extend(player_positions)

    # Shuffle for better training
    random.shuffle(all_positions)

    print(f"\n  Total: {len(all_positions)} quiet positions")

    # Count results
    wins = sum(1 for p in all_positions if p.endswith("1.0"))
    draws = sum(1 for p in all_positions if p.endswith("0.5"))
    losses = sum(1 for p in all_positions if p.endswith("0.0"))
    print(f"  Distribution: {wins} white wins, {draws} draws, {losses} white losses")

    with open(OUTPUT, "w") as f:
        for line in all_positions:
            f.write(line + "\n")

    print(f"  Saved to {OUTPUT}")
    print(f"  File size: {os.path.getsize(OUTPUT) / 1024 / 1024:.1f} MB")


if __name__ == "__main__":
    main()
