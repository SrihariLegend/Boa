#!/usr/bin/env python3
"""
Karpov Game Analyzer — Compute 50 positional metrics on master games.

Analyzes games from Karpov, Petrosian, and Keres to discover which
positional metrics correlate with each player's style and with winning.

Usage:
    python3 tools/game_analyzer.py [--sample-range 10 40] [--max-games 0]

Output:
    tools/analysis_results/metrics.csv     — raw per-position metrics
    tools/analysis_results/report.txt      — statistical summary
    tools/analysis_results/player_profiles.csv
"""

import chess
import chess.pgn
import csv
import io
import math
import os
import sys
import zipfile
import argparse
import time
from concurrent.futures import ProcessPoolExecutor, as_completed
from collections import defaultdict
from dataclasses import dataclass, fields, asdict

# ── Colors ───────────────────────────────────────────────────
GREEN  = "\033[92m"
RED    = "\033[91m"
YELLOW = "\033[93m"
CYAN   = "\033[96m"
BOLD   = "\033[1m"
DIM    = "\033[2m"
RESET  = "\033[0m"

DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GAMES_DIR = os.path.join(DIR, "games")
OUTPUT_DIR = os.path.join(DIR, "tools", "analysis_results")


# ============================================================
# Metric data structure
# ============================================================

@dataclass
class PositionMetrics:
    # Identifiers
    player: str = ""
    game_idx: int = 0
    move_num: int = 0
    result: str = ""  # "W", "D", "L" from player's perspective
    ply: int = 0

    # Mobility & Freedom (1-8)
    our_legal_moves: int = 0
    opp_legal_moves: int = 0
    mobility_ratio: float = 0.0
    mobility_delta: int = 0
    piece_mobility: int = 0
    pawn_mobility: int = 0
    mobility_change: int = 0  # change in opp mobility from previous position
    min_piece_mobility: int = 0

    # Space & Territory (9-14)
    space_count: int = 0
    advanced_space: int = 0
    center_control: int = 0
    extended_center_control: int = 0
    territory_behind_pawns: int = 0
    pawn_chain_length: int = 0

    # Pawn Structure (15-22)
    isolated_pawns: int = 0
    doubled_pawns: int = 0
    backward_pawns: int = 0
    passed_pawns: int = 0
    protected_passed_pawns: int = 0
    pawn_islands: int = 0
    our_pawn_breaks: int = 0
    opp_pawn_breaks: int = 0

    # Piece Activity & Coordination (23-32)
    pieces_defending_each_other: int = 0
    attack_overlap: int = 0
    pieces_on_outposts: int = 0
    knights_on_holes: int = 0
    rook_open_file: int = 0
    rook_semi_open_file: int = 0
    rook_behind_passer: int = 0
    bishop_pair: int = 0
    bad_bishop_score: int = 0
    minor_piece_imbalance: int = 0  # +1 = we have N vs B advantage in closed, etc.

    # King Safety (33-38)
    pawn_shield_count: int = 0
    king_zone_attackers: int = 0
    king_zone_attack_weight: int = 0
    king_exposure: int = 0
    king_centralization: int = 0
    castling_status: int = 0  # 2=castled, 1=can castle, 0=lost rights

    # Tactical Tension (39-44)
    our_hanging_pieces: int = 0
    opp_hanging_pieces: int = 0
    pieces_en_prise: int = 0
    pin_count: int = 0
    check_available: int = 0
    fork_potential: int = 0

    # Material & Phase (45-48)
    material_balance: int = 0
    non_pawn_material: int = 0
    material_imbalance_type: int = 0  # 0=equal, 1=exchange up, -1=exchange down, 2=minor up, etc.
    pawn_count_ratio: float = 0.0

    # Strategic (49-50)
    restriction_score: int = 0
    squeeze_index: int = 0  # 1 if opp mobility <= 12


# ============================================================
# Helpers
# ============================================================

PIECE_VALUES = {
    chess.PAWN: 100, chess.KNIGHT: 320, chess.BISHOP: 330,
    chess.ROOK: 500, chess.QUEEN: 900, chess.KING: 0
}

CENTER_SQUARES = [chess.D4, chess.D5, chess.E4, chess.E5]
EXTENDED_CENTER = [
    chess.C3, chess.D3, chess.E3, chess.F3,
    chess.C4, chess.D4, chess.E4, chess.F4,
    chess.C5, chess.D5, chess.E5, chess.F5,
    chess.C6, chess.D6, chess.E6, chess.F6,
]

KING_ATTACK_WEIGHTS = {
    chess.KNIGHT: 2, chess.BISHOP: 2, chess.ROOK: 3, chess.QUEEN: 5
}


def attackers_mask(board, color, square):
    """Get all pieces of `color` attacking `square`."""
    return board.attackers(color, square)


def count_attacks_on_squares(board, color, squares):
    """Count how many of the given squares are attacked by `color`."""
    return sum(1 for sq in squares if board.is_attacked_by(color, sq))


def get_pawn_files(board, color):
    """Return set of files that have pawns of this color."""
    pawns = board.pieces(chess.PAWN, color)
    return set(chess.square_file(sq) for sq in pawns)


def is_passed_pawn(board, sq, color):
    """Check if pawn at sq is a passed pawn."""
    file = chess.square_file(sq)
    rank = chess.square_rank(sq)
    opp = not color
    opp_pawns = board.pieces(chess.PAWN, opp)

    for f in [file - 1, file, file + 1]:
        if f < 0 or f > 7:
            continue
        for opp_sq in opp_pawns:
            opp_file = chess.square_file(opp_sq)
            opp_rank = chess.square_rank(opp_sq)
            if opp_file != f:
                continue
            if color == chess.WHITE and opp_rank > rank:
                return False
            if color == chess.BLACK and opp_rank < rank:
                return False
    return True


def pawn_attacks(board, color):
    """Get all squares attacked by pawns of this color."""
    attacked = chess.SquareSet()
    for sq in board.pieces(chess.PAWN, color):
        attacked |= board.attacks(sq)
    return attacked


def count_pawn_islands(board, color):
    """Count pawn islands (groups of connected files with pawns)."""
    files = sorted(get_pawn_files(board, color))
    if not files:
        return 0
    islands = 1
    for i in range(1, len(files)):
        if files[i] > files[i - 1] + 1:
            islands += 1
    return islands


def king_zone(king_sq):
    """Squares around the king (king moves + king square itself)."""
    zones = chess.SquareSet()
    zones.add(king_sq)
    for delta_r in [-1, 0, 1]:
        for delta_f in [-1, 0, 1]:
            f = chess.square_file(king_sq) + delta_f
            r = chess.square_rank(king_sq) + delta_r
            if 0 <= f <= 7 and 0 <= r <= 7:
                zones.add(chess.square(f, r))
    return zones


def longest_pawn_chain(board, color):
    """Find the longest diagonal pawn chain for a color."""
    pawns = list(board.pieces(chess.PAWN, color))
    if not pawns:
        return 0

    pawn_set = set(pawns)
    visited = set()
    max_chain = 0

    def chain_len(sq, visited_local):
        f, r = chess.square_file(sq), chess.square_rank(sq)
        best = 1
        # Pawn chain: diagonally connected (defended by another pawn)
        for df in [-1, 1]:
            dr = 1 if color == chess.WHITE else -1
            nf, nr = f + df, r + dr
            if 0 <= nf <= 7 and 0 <= nr <= 7:
                nsq = chess.square(nf, nr)
                if nsq in pawn_set and nsq not in visited_local:
                    visited_local.add(nsq)
                    best = max(best, 1 + chain_len(nsq, visited_local))
        return best

    for sq in pawns:
        if sq not in visited:
            local_visited = {sq}
            cl = chain_len(sq, local_visited)
            visited |= local_visited
            max_chain = max(max_chain, cl)

    return max_chain


def compute_piece_mobility(board, color):
    """Count legal squares for non-pawn, non-king pieces."""
    total = 0
    min_mob = 999
    has_piece = False

    for pt in [chess.KNIGHT, chess.BISHOP, chess.ROOK, chess.QUEEN]:
        for sq in board.pieces(pt, color):
            has_piece = True
            mob = len(board.attacks(sq) - board.pieces(chess.PAWN, color) - chess.SquareSet([board.king(color)] if board.king(color) is not None else []))
            total += mob
            min_mob = min(min_mob, mob)

    return total, (min_mob if has_piece else 0)


def count_pawn_mobility(board, color):
    """Count available pawn moves (pushes + captures)."""
    count = 0
    for sq in board.pieces(chess.PAWN, color):
        # Check pushes
        if color == chess.WHITE:
            fwd = sq + 8
            if fwd < 64 and board.piece_at(fwd) is None:
                count += 1
                if chess.square_rank(sq) == 1:
                    fwd2 = sq + 16
                    if fwd2 < 64 and board.piece_at(fwd2) is None:
                        count += 1
        else:
            fwd = sq - 8
            if fwd >= 0 and board.piece_at(fwd) is None:
                count += 1
                if chess.square_rank(sq) == 6:
                    fwd2 = sq - 16
                    if fwd2 >= 0 and board.piece_at(fwd2) is None:
                        count += 1

        # Captures
        attacks = board.attacks(sq)
        opp_pieces = board.occupied_co[not color]
        count += len(attacks & opp_pieces)

    return count


def count_pawn_breaks(board, color):
    """Count pawns that can advance to challenge opponent pawn structure."""
    opp = not color
    opp_pawns = board.pieces(chess.PAWN, opp)
    opp_files = set(chess.square_file(sq) for sq in opp_pawns)
    breaks = 0

    for sq in board.pieces(chess.PAWN, color):
        f = chess.square_file(sq)
        # Is this pawn on an adjacent file to opponent pawns?
        if (f - 1) not in opp_files and (f + 1) not in opp_files:
            continue
        # Can it advance?
        advance = sq + (8 if color == chess.WHITE else -8)
        if 0 <= advance < 64 and board.piece_at(advance) is None:
            breaks += 1

    return breaks


def is_backward_pawn(board, sq, color):
    """Check if pawn at sq is backward."""
    f = chess.square_file(sq)
    r = chess.square_rank(sq)
    opp = not color

    # Check advance square attacked by opponent pawn
    advance = sq + (8 if color == chess.WHITE else -8)
    if advance < 0 or advance >= 64:
        return False

    if not board.is_attacked_by(opp, advance):
        return False

    # Check no friendly pawn on adjacent files at or behind same rank
    for adj_f in [f - 1, f + 1]:
        if adj_f < 0 or adj_f > 7:
            continue
        for adj_sq in board.pieces(chess.PAWN, color):
            adj_sq_f = chess.square_file(adj_sq)
            adj_sq_r = chess.square_rank(adj_sq)
            if adj_sq_f != adj_f:
                continue
            if color == chess.WHITE and adj_sq_r <= r:
                return False
            if color == chess.BLACK and adj_sq_r >= r:
                return False

    return True


def material_value(board, color):
    """Total material value for a color."""
    total = 0
    for pt in [chess.PAWN, chess.KNIGHT, chess.BISHOP, chess.ROOK, chess.QUEEN]:
        total += len(board.pieces(pt, color)) * PIECE_VALUES[pt]
    return total


def non_pawn_material_value(board, color):
    """Non-pawn material value."""
    total = 0
    for pt in [chess.KNIGHT, chess.BISHOP, chess.ROOK, chess.QUEEN]:
        total += len(board.pieces(pt, color)) * PIECE_VALUES[pt]
    return total


def detect_pins(board, color):
    """Count pieces of `color` that are pinned."""
    pin_count = 0
    king_sq = board.king(color)
    if king_sq is None:
        return 0
    for sq in chess.SQUARES:
        piece = board.piece_at(sq)
        if piece and piece.color == color and sq != king_sq:
            if board.is_pinned(color, sq):
                pin_count += 1
    return pin_count


def count_hanging(board, color):
    """Count pieces of `color` that are attacked and not defended."""
    hanging = 0
    opp = not color
    for pt in [chess.KNIGHT, chess.BISHOP, chess.ROOK, chess.QUEEN]:
        for sq in board.pieces(pt, color):
            if board.is_attacked_by(opp, sq) and not board.is_attacked_by(color, sq):
                hanging += 1
    return hanging


def count_en_prise(board, color):
    """Count pieces of `color` attacked by lower-value enemy pieces."""
    en_prise = 0
    opp = not color
    for pt in [chess.KNIGHT, chess.BISHOP, chess.ROOK, chess.QUEEN]:
        for sq in board.pieces(pt, color):
            if not board.is_attacked_by(opp, sq):
                continue
            piece_val = PIECE_VALUES[pt]
            # Check if any attacker has lower value
            for attacker_sq in board.attackers(opp, sq):
                attacker = board.piece_at(attacker_sq)
                if attacker and PIECE_VALUES.get(attacker.piece_type, 0) < piece_val:
                    en_prise += 1
                    break
    return en_prise


def count_fork_potential(board, color):
    """Count knight/pawn squares that attack 2+ enemy pieces."""
    forks = 0
    for pt in [chess.KNIGHT, chess.PAWN]:
        for sq in board.pieces(pt, color):
            attacks = board.attacks(sq)
            opp_pieces_attacked = 0
            for target in attacks:
                piece = board.piece_at(target)
                if piece and piece.color != color and piece.piece_type != chess.PAWN:
                    opp_pieces_attacked += 1
            if opp_pieces_attacked >= 2:
                forks += 1
    return forks


def check_available(board, color):
    """Can any piece of `color` give check?"""
    opp_king = board.king(not color)
    if opp_king is None:
        return 0
    for sq in chess.SQUARES:
        piece = board.piece_at(sq)
        if piece and piece.color == color:
            if opp_king in board.attacks(sq):
                return 1
    return 0


def space_behind_pawns(board, color):
    """Count squares behind our pawn chain that we control."""
    count = 0
    our_pawns = board.pieces(chess.PAWN, color)
    center_files = {2, 3, 4, 5}  # C-F

    for sq in our_pawns:
        f = chess.square_file(sq)
        if f not in center_files:
            continue
        r = chess.square_rank(sq)
        # Squares behind this pawn
        if color == chess.WHITE:
            for br in range(1, r):
                behind_sq = chess.square(f, br)
                if board.piece_at(behind_sq) is None:
                    count += 1
        else:
            for br in range(r + 1, 7):
                behind_sq = chess.square(f, br)
                if board.piece_at(behind_sq) is None:
                    count += 1
    return count


def outpost_squares(board, color):
    """Find squares on ranks 4-6 (our perspective) not attackable by enemy pawns."""
    opp = not color
    opp_pawn_attacks = pawn_attacks(board, opp)
    outpost_ranks = range(3, 6) if color == chess.WHITE else range(2, 5)

    outposts = chess.SquareSet()
    for sq in chess.SQUARES:
        r = chess.square_rank(sq)
        if r not in outpost_ranks:
            continue
        if sq in opp_pawn_attacks:
            continue

        # Check if any enemy pawn on adjacent files could ever defend this square
        f = chess.square_file(sq)
        can_be_defended = False
        for adj_f in [f - 1, f + 1]:
            if adj_f < 0 or adj_f > 7:
                continue
            for opp_sq in board.pieces(chess.PAWN, opp):
                opp_f = chess.square_file(opp_sq)
                opp_r = chess.square_rank(opp_sq)
                if opp_f != adj_f:
                    continue
                if color == chess.WHITE and opp_r > r:
                    can_be_defended = True
                if color == chess.BLACK and opp_r < r:
                    can_be_defended = True

        if not can_be_defended:
            outposts.add(sq)

    return outposts


def king_exposure_score(board, color):
    """Count open/semi-open lines toward our king."""
    king_sq = board.king(color)
    if king_sq is None:
        return 0
    king_file = chess.square_file(king_sq)
    exposure = 0
    our_pawns = board.pieces(chess.PAWN, color)

    # Check files around king
    for f in [king_file - 1, king_file, king_file + 1]:
        if f < 0 or f > 7:
            continue
        has_our_pawn = any(chess.square_file(sq) == f for sq in our_pawns)
        if not has_our_pawn:
            exposure += 1

    return exposure


def pawn_shield(board, color):
    """Count pawns in front of king (shield)."""
    king_sq = board.king(color)
    if king_sq is None:
        return 0

    king_file = chess.square_file(king_sq)
    king_rank = chess.square_rank(king_sq)
    shield_count = 0

    shield_ranks = [king_rank + 1, king_rank + 2] if color == chess.WHITE else [king_rank - 1, king_rank - 2]

    for f in [king_file - 1, king_file, king_file + 1]:
        if f < 0 or f > 7:
            continue
        for r in shield_ranks:
            if r < 0 or r > 7:
                continue
            sq = chess.square(f, r)
            piece = board.piece_at(sq)
            if piece and piece.piece_type == chess.PAWN and piece.color == color:
                shield_count += 1
                break  # Only count nearest pawn per file

    return shield_count


def castling_status_score(board, color):
    """2=castled, 1=can castle, 0=lost rights."""
    king_sq = board.king(color)
    if king_sq is None:
        return 0

    # Check if already castled (king on g1/c1 for white, g8/c8 for black)
    if color == chess.WHITE:
        if king_sq in [chess.G1, chess.C1]:
            return 2
    else:
        if king_sq in [chess.G8, chess.C8]:
            return 2

    # Check if can still castle
    if color == chess.WHITE:
        if board.has_kingside_castling_rights(chess.WHITE) or board.has_queenside_castling_rights(chess.WHITE):
            return 1
    else:
        if board.has_kingside_castling_rights(chess.BLACK) or board.has_queenside_castling_rights(chess.BLACK):
            return 1

    return 0


def material_imbalance(board, color):
    """Classify material imbalance. 0=equal, 1=exchange up, -1=exchange down, 2=minor up, -2=minor down."""
    opp = not color
    our_r = len(board.pieces(chess.ROOK, color))
    opp_r = len(board.pieces(chess.ROOK, opp))
    our_minor = len(board.pieces(chess.KNIGHT, color)) + len(board.pieces(chess.BISHOP, color))
    opp_minor = len(board.pieces(chess.KNIGHT, opp)) + len(board.pieces(chess.BISHOP, opp))

    # Exchange imbalance: R vs minor
    if our_r > opp_r and our_minor < opp_minor:
        return 1  # exchange up
    if our_r < opp_r and our_minor > opp_minor:
        return -1  # exchange down

    mat_diff = material_value(board, color) - material_value(board, opp)
    if mat_diff > 250:
        return 2  # minor piece up
    if mat_diff < -250:
        return -2  # minor piece down

    return 0


def pieces_defending_each_other_count(board, color):
    """Count our non-pawn pieces defended by another of our non-pawn pieces."""
    count = 0
    for pt in [chess.KNIGHT, chess.BISHOP, chess.ROOK, chess.QUEEN]:
        for sq in board.pieces(pt, color):
            if board.is_attacked_by(color, sq):
                # Check if defended by a non-pawn piece
                for def_sq in board.attackers(color, sq):
                    def_piece = board.piece_at(def_sq)
                    if def_piece and def_piece.piece_type != chess.PAWN and def_piece.piece_type != chess.KING:
                        count += 1
                        break
    return count


def attack_overlap_count(board, color):
    """Count squares attacked by 2+ of our pieces."""
    attack_counts = defaultdict(int)
    for sq in chess.SQUARES:
        piece = board.piece_at(sq)
        if piece and piece.color == color and piece.piece_type != chess.KING:
            for target in board.attacks(sq):
                attack_counts[target] += 1

    return sum(1 for sq, cnt in attack_counts.items() if cnt >= 2)


def king_zone_attacks(board, color):
    """Count opponent pieces attacking our king zone, with weights."""
    opp = not color
    king_sq = board.king(color)
    if king_sq is None:
        return 0, 0

    kzone = king_zone(king_sq)
    attackers = 0
    weight = 0

    for pt in [chess.KNIGHT, chess.BISHOP, chess.ROOK, chess.QUEEN]:
        for sq in board.pieces(pt, opp):
            attacks = board.attacks(sq)
            if attacks & kzone:
                attackers += 1
                weight += KING_ATTACK_WEIGHTS.get(pt, 1)

    return attackers, weight


# ============================================================
# Main metric computation
# ============================================================

def compute_metrics(board, color, prev_opp_mobility=None):
    """Compute all 50 metrics for the position from `color`'s perspective."""
    m = PositionMetrics()
    opp = not color

    # --- Mobility & Freedom (1-8) ---
    # We need legal moves for both sides. python-chess only gives legal moves
    # for the side to move. We'll use pseudo-legal attacks as proxy for opponent.
    if board.turn == color:
        m.our_legal_moves = len(list(board.legal_moves))
    else:
        m.our_legal_moves = len(list(board.pseudo_legal_moves))

    # For opponent mobility, push a null move if possible, otherwise estimate
    board_copy = board.copy()
    board_copy.push(chess.Move.null())
    m.opp_legal_moves = len(list(board_copy.legal_moves))
    board_copy.pop()

    m.mobility_ratio = m.our_legal_moves / max(m.opp_legal_moves, 1)
    m.mobility_delta = m.our_legal_moves - m.opp_legal_moves

    pm, min_mob = compute_piece_mobility(board, color)
    m.piece_mobility = pm
    m.min_piece_mobility = min_mob
    m.pawn_mobility = count_pawn_mobility(board, color)

    if prev_opp_mobility is not None:
        m.mobility_change = prev_opp_mobility - m.opp_legal_moves
    else:
        m.mobility_change = 0

    # --- Space & Territory (9-14) ---
    m.center_control = count_attacks_on_squares(board, color, CENTER_SQUARES)
    m.extended_center_control = count_attacks_on_squares(board, color, EXTENDED_CENTER)

    # Space: squares on our half that we attack
    our_half = range(0, 32) if color == chess.WHITE else range(32, 64)
    m.space_count = sum(1 for sq in our_half if board.is_attacked_by(color, sq))

    # Advanced space: ranks 5-7 from our perspective
    adv_ranks = range(4, 7) if color == chess.WHITE else range(1, 4)
    m.advanced_space = sum(
        1 for sq in chess.SQUARES
        if chess.square_rank(sq) in adv_ranks and board.is_attacked_by(color, sq)
    )

    m.territory_behind_pawns = space_behind_pawns(board, color)
    m.pawn_chain_length = longest_pawn_chain(board, color)

    # --- Pawn Structure (15-22) ---
    our_pawns = board.pieces(chess.PAWN, color)
    opp_pawns = board.pieces(chess.PAWN, opp)
    our_pawn_files = get_pawn_files(board, color)

    for sq in our_pawns:
        f = chess.square_file(sq)
        # Doubled
        same_file = [s for s in our_pawns if chess.square_file(s) == f and s != sq]
        if same_file:
            m.doubled_pawns += 1

        # Isolated
        adj = set()
        if f > 0: adj.add(f - 1)
        if f < 7: adj.add(f + 1)
        is_isolated = not adj.intersection(our_pawn_files)
        if is_isolated:
            m.isolated_pawns += 1

        # Backward
        if not is_isolated and is_backward_pawn(board, sq, color):
            m.backward_pawns += 1

        # Passed
        if is_passed_pawn(board, sq, color):
            m.passed_pawns += 1
            # Protected passed
            if board.is_attacked_by(color, sq):
                pawn_defenders = [
                    s for s in board.attackers(color, sq)
                    if board.piece_at(s) and board.piece_at(s).piece_type == chess.PAWN
                ]
                if pawn_defenders:
                    m.protected_passed_pawns += 1

    # Avoid double-counting doubled pawns (counted per-pawn, not per-pair)
    m.doubled_pawns = m.doubled_pawns // 2

    m.pawn_islands = count_pawn_islands(board, color)
    m.our_pawn_breaks = count_pawn_breaks(board, color)
    m.opp_pawn_breaks = count_pawn_breaks(board, opp)

    # --- Piece Activity & Coordination (23-32) ---
    m.pieces_defending_each_other = pieces_defending_each_other_count(board, color)
    m.attack_overlap = attack_overlap_count(board, color)

    outposts = outpost_squares(board, color)
    m.pieces_on_outposts = sum(
        1 for sq in outposts
        if board.piece_at(sq) and board.piece_at(sq).color == color
        and board.piece_at(sq).piece_type not in [chess.PAWN, chess.KING]
    )
    m.knights_on_holes = sum(
        1 for sq in outposts
        if board.piece_at(sq) and board.piece_at(sq).color == color
        and board.piece_at(sq).piece_type == chess.KNIGHT
    )

    # Rook on open/semi-open file
    for sq in board.pieces(chess.ROOK, color):
        f = chess.square_file(sq)
        our_pawn_on_file = any(chess.square_file(s) == f for s in our_pawns)
        opp_pawn_on_file = any(chess.square_file(s) == f for s in opp_pawns)
        if not our_pawn_on_file:
            if not opp_pawn_on_file:
                m.rook_open_file += 1
            else:
                m.rook_semi_open_file += 1

    # Rook behind passer
    for sq in our_pawns:
        if is_passed_pawn(board, sq, color):
            f = chess.square_file(sq)
            r = chess.square_rank(sq)
            for rsq in board.pieces(chess.ROOK, color):
                rf = chess.square_file(rsq)
                rr = chess.square_rank(rsq)
                if rf == f:
                    if color == chess.WHITE and rr < r:
                        m.rook_behind_passer += 1
                    elif color == chess.BLACK and rr > r:
                        m.rook_behind_passer += 1

    m.bishop_pair = 1 if len(board.pieces(chess.BISHOP, color)) >= 2 else 0

    # Bad bishop: own pawns on same color as bishop
    for bsq in board.pieces(chess.BISHOP, color):
        bishop_light = (chess.square_file(bsq) + chess.square_rank(bsq)) % 2
        blocking = sum(
            1 for psq in our_pawns
            if (chess.square_file(psq) + chess.square_rank(psq)) % 2 == bishop_light
        )
        m.bad_bishop_score += max(0, blocking - 2)

    # Minor piece imbalance
    our_knights = len(board.pieces(chess.KNIGHT, color))
    our_bishops = len(board.pieces(chess.BISHOP, color))
    opp_knights = len(board.pieces(chess.KNIGHT, opp))
    opp_bishops = len(board.pieces(chess.BISHOP, opp))
    center_pawns = sum(1 for sq in [chess.D4, chess.D5, chess.E4, chess.E5]
                       if board.piece_at(sq) and board.piece_at(sq).piece_type == chess.PAWN)
    if center_pawns >= 4 and our_knights > 0 and our_bishops == 0 and opp_bishops > 0 and opp_knights == 0:
        m.minor_piece_imbalance = 1
    elif center_pawns >= 4 and opp_knights > 0 and opp_bishops == 0 and our_bishops > 0 and our_knights == 0:
        m.minor_piece_imbalance = -1
    else:
        m.minor_piece_imbalance = 0

    # --- King Safety (33-38) ---
    m.pawn_shield_count = pawn_shield(board, color)
    m.king_zone_attackers, m.king_zone_attack_weight = king_zone_attacks(board, color)
    m.king_exposure = king_exposure_score(board, color)

    king_sq = board.king(color)
    if king_sq is not None:
        kf, kr = chess.square_file(king_sq), chess.square_rank(king_sq)
        m.king_centralization = 7 - (abs(3 - kf) + abs(3 - kr))  # higher = more central

    m.castling_status = castling_status_score(board, color)

    # --- Tactical Tension (39-44) ---
    m.our_hanging_pieces = count_hanging(board, color)
    m.opp_hanging_pieces = count_hanging(board, opp)
    m.pieces_en_prise = count_en_prise(board, color)
    m.pin_count = detect_pins(board, color)
    m.check_available = check_available(board, color)
    m.fork_potential = count_fork_potential(board, color)

    # --- Material & Phase (45-48) ---
    our_mat = material_value(board, color)
    opp_mat = material_value(board, opp)
    m.material_balance = our_mat - opp_mat
    m.non_pawn_material = non_pawn_material_value(board, color) + non_pawn_material_value(board, opp)
    m.material_imbalance_type = material_imbalance(board, color)
    our_pawn_count = len(our_pawns)
    opp_pawn_count = len(opp_pawns)
    m.pawn_count_ratio = our_pawn_count / max(opp_pawn_count, 1)

    # --- Strategic (49-50) ---
    m.restriction_score = m.mobility_delta + m.advanced_space - m.opp_pawn_breaks
    m.squeeze_index = 1 if m.opp_legal_moves <= 12 else 0

    return m


# ============================================================
# Game processing
# ============================================================

def process_game(game, player_name, game_idx, sample_start, sample_end):
    """Process a single game, returning list of PositionMetrics."""
    # Determine which color the player is
    white = game.headers.get("White", "")
    black = game.headers.get("Black", "")
    result_str = game.headers.get("Result", "*")

    if player_name.lower() in white.lower():
        player_color = chess.WHITE
    elif player_name.lower() in black.lower():
        player_color = chess.BLACK
    else:
        return []  # Player not found in this game

    # Determine result from player's perspective
    if result_str == "1-0":
        result = "W" if player_color == chess.WHITE else "L"
    elif result_str == "0-1":
        result = "L" if player_color == chess.WHITE else "W"
    elif result_str == "1/2-1/2":
        result = "D"
    else:
        return []  # Unknown result

    board = game.board()
    metrics_list = []
    prev_opp_mobility = None
    move_num = 0

    for node in game.mainline():
        move = node.move
        board.push(move)
        move_num += 1

        # Only sample middlegame positions (configurable range)
        if move_num < sample_start or move_num > sample_end:
            prev_opp_mobility = None
            continue

        # Only analyze when it's the player's turn (they're about to move)
        if board.turn != player_color:
            continue

        try:
            m = compute_metrics(board, player_color, prev_opp_mobility)
            m.player = player_name
            m.game_idx = game_idx
            m.move_num = move_num
            m.result = result
            m.ply = board.ply()

            # Track opponent mobility for next iteration
            prev_opp_mobility = m.opp_legal_moves

            metrics_list.append(m)
        except Exception:
            continue

    return metrics_list


def load_games(zip_path, player_name, max_games=0):
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


# ============================================================
# Statistical analysis
# ============================================================

def analyze_results(all_metrics):
    """Perform statistical analysis on collected metrics."""
    # Group by player
    by_player = defaultdict(list)
    for m in all_metrics:
        by_player[m.player].append(m)

    # Get metric field names (skip identifiers)
    skip = {"player", "game_idx", "move_num", "result", "ply"}
    metric_names = [f.name for f in fields(PositionMetrics) if f.name not in skip]

    # ── Player profiles ──
    profiles = {}
    for player, data in by_player.items():
        profile = {}
        for name in metric_names:
            values = [getattr(m, name) for m in data]
            if values:
                profile[name] = {
                    "mean": sum(values) / len(values),
                    "median": sorted(values)[len(values) // 2],
                    "std": math.sqrt(sum((v - sum(values)/len(values))**2 for v in values) / max(len(values)-1, 1)),
                }
            else:
                profile[name] = {"mean": 0, "median": 0, "std": 0}
        profiles[player] = profile

    # ── Win correlation ──
    win_corr = {}
    for name in metric_names:
        # Across all players combined
        wins = [getattr(m, name) for m in all_metrics if m.result == "W"]
        losses = [getattr(m, name) for m in all_metrics if m.result == "L"]
        draws = [getattr(m, name) for m in all_metrics if m.result == "D"]

        win_mean = sum(wins) / max(len(wins), 1)
        loss_mean = sum(losses) / max(len(losses), 1)
        draw_mean = sum(draws) / max(len(draws), 1)

        # Simple effect size: (win_mean - loss_mean) / pooled_std
        all_vals = wins + losses
        if len(all_vals) > 1:
            overall_mean = sum(all_vals) / len(all_vals)
            pooled_std = math.sqrt(sum((v - overall_mean)**2 for v in all_vals) / (len(all_vals) - 1))
        else:
            pooled_std = 1

        effect_size = (win_mean - loss_mean) / max(pooled_std, 0.01)

        # Mann-Whitney approximation: just use rank comparison
        # Simple: what fraction of win values > loss values
        if wins and losses:
            count_greater = sum(1 for w in wins for l in losses if w > l)
            auc = count_greater / (len(wins) * len(losses))
        else:
            auc = 0.5

        win_corr[name] = {
            "win_mean": win_mean,
            "loss_mean": loss_mean,
            "draw_mean": draw_mean,
            "effect_size": effect_size,
            "auc": auc,  # 0.5 = random, >0.5 = higher values → more wins
        }

    # ── Player discrimination ──
    # For each metric, how much does it separate players?
    discrimination = {}
    players = list(by_player.keys())
    for name in metric_names:
        max_diff = 0
        for i in range(len(players)):
            for j in range(i + 1, len(players)):
                p1 = profiles[players[i]][name]["mean"]
                p2 = profiles[players[j]][name]["mean"]
                std1 = profiles[players[i]][name]["std"]
                std2 = profiles[players[j]][name]["std"]
                pooled = math.sqrt((std1**2 + std2**2) / 2) if (std1 + std2) > 0 else 1
                d = abs(p1 - p2) / max(pooled, 0.01)
                max_diff = max(max_diff, d)
        discrimination[name] = max_diff

    return profiles, win_corr, discrimination, metric_names


# ============================================================
# Reporting
# ============================================================

def print_report(profiles, win_corr, discrimination, metric_names, all_metrics):
    """Print a formatted report."""
    players = list(profiles.keys())

    print()
    print(f"{BOLD}{'═' * 90}{RESET}")
    print(f"{BOLD}{CYAN}  Karpov Game Analyzer — Master Game Metric Analysis{RESET}")
    print(f"{BOLD}{'═' * 90}{RESET}")

    # Game counts
    by_player = defaultdict(lambda: {"W": 0, "D": 0, "L": 0, "total": 0})
    seen_games = defaultdict(set)
    for m in all_metrics:
        if m.game_idx not in seen_games[m.player]:
            seen_games[m.player].add(m.game_idx)
            by_player[m.player]["total"] += 1
            by_player[m.player][m.result] += 1

    for p in players:
        s = by_player[p]
        print(f"  {BOLD}{p}{RESET}: {s['total']} games ({s['W']}W {s['D']}D {s['L']}L), "
              f"{sum(1 for m in all_metrics if m.player == p)} positions sampled")

    # ── Top Metrics by Win Prediction ──
    print(f"\n{BOLD}{'─' * 90}{RESET}")
    print(f"{BOLD}{CYAN}  Top 20 Metrics by Win Prediction (effect size: win_mean − loss_mean / std){RESET}")
    print(f"{BOLD}{'─' * 90}{RESET}")
    print(f"  {'Metric':<30} {'Effect':>7} {'AUC':>5}  {'Win μ':>8} {'Loss μ':>8} {'Draw μ':>8}")
    print(f"  {'─' * 80}")

    sorted_win = sorted(metric_names, key=lambda n: abs(win_corr[n]["effect_size"]), reverse=True)
    for name in sorted_win[:20]:
        wc = win_corr[name]
        eff = wc["effect_size"]
        color = GREEN if eff > 0.1 else RED if eff < -0.1 else ""
        reset = RESET if color else ""
        print(f"  {color}{name:<30} {eff:>+7.3f} {wc['auc']:>5.3f}  {wc['win_mean']:>8.2f} {wc['loss_mean']:>8.2f} {wc['draw_mean']:>8.2f}{reset}")

    # ── Top Metrics by Player Discrimination ──
    print(f"\n{BOLD}{'─' * 90}{RESET}")
    print(f"{BOLD}{CYAN}  Top 20 Metrics by Player Style Discrimination (Cohen's d between players){RESET}")
    print(f"{BOLD}{'─' * 90}{RESET}")
    print(f"  {'Metric':<30} {'Max d':>7}  ", end="")
    for p in players:
        print(f"{p[:8]:>10}", end="")
    print()
    print(f"  {'─' * 80}")

    sorted_disc = sorted(metric_names, key=lambda n: discrimination[n], reverse=True)
    for name in sorted_disc[:20]:
        d = discrimination[name]
        print(f"  {name:<30} {d:>7.3f}  ", end="")
        for p in players:
            val = profiles[p][name]["mean"]
            print(f"{val:>10.2f}", end="")
        print()

    # ── Combined Score ──
    print(f"\n{BOLD}{'─' * 90}{RESET}")
    print(f"{BOLD}{CYAN}  Combined Metric Ranking (40% win prediction + 30% discrimination + 30% AUC){RESET}")
    print(f"{BOLD}{'─' * 90}{RESET}")

    # Normalize scores
    max_eff = max(abs(win_corr[n]["effect_size"]) for n in metric_names) or 1
    max_disc = max(discrimination[n] for n in metric_names) or 1

    combined = {}
    for name in metric_names:
        eff_norm = abs(win_corr[name]["effect_size"]) / max_eff
        disc_norm = discrimination[name] / max_disc
        auc_norm = abs(win_corr[name]["auc"] - 0.5) * 2  # 0-1 scale
        combined[name] = 0.4 * eff_norm + 0.3 * disc_norm + 0.3 * auc_norm

    sorted_combined = sorted(metric_names, key=lambda n: combined[n], reverse=True)
    print(f"  {'Rank':<5} {'Metric':<30} {'Score':>7} {'WinEff':>7} {'Discr':>7} {'AUC':>7}")
    print(f"  {'─' * 70}")
    for i, name in enumerate(sorted_combined[:25]):
        sc = combined[name]
        wc = win_corr[name]
        dc = discrimination[name]
        color = GREEN if sc > 0.5 else YELLOW if sc > 0.3 else ""
        reset = RESET if color else ""
        print(f"  {color}{i+1:<5} {name:<30} {sc:>7.3f} {wc['effect_size']:>+7.3f} {dc:>7.3f} {wc['auc']:>7.3f}{reset}")

    # ── Player Style Radar (text version) ──
    print(f"\n{BOLD}{'─' * 90}{RESET}")
    print(f"{BOLD}{CYAN}  Player Style Signatures (top discriminating metrics, normalized){RESET}")
    print(f"{BOLD}{'─' * 90}{RESET}")

    # Show top 15 discriminating metrics as a text "radar"
    for name in sorted_disc[:15]:
        vals = {p: profiles[p][name]["mean"] for p in players}
        max_val = max(abs(v) for v in vals.values()) or 1
        print(f"\n  {BOLD}{name}{RESET}")
        for p in players:
            bar_len = int(abs(vals[p]) / max_val * 40)
            bar = "█" * bar_len
            print(f"    {p[:12]:<12} {vals[p]:>8.2f} {bar}")

    print(f"\n{BOLD}{'═' * 90}{RESET}")


def save_csv(all_metrics, filepath):
    """Save all metrics to CSV."""
    os.makedirs(os.path.dirname(filepath), exist_ok=True)
    field_names = [f.name for f in fields(PositionMetrics)]
    with open(filepath, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=field_names)
        writer.writeheader()
        for m in all_metrics:
            writer.writerow(asdict(m))


def save_profiles(profiles, filepath):
    """Save player profiles to CSV."""
    os.makedirs(os.path.dirname(filepath), exist_ok=True)
    skip = {"player", "game_idx", "move_num", "result", "ply"}
    metric_names = [f.name for f in fields(PositionMetrics) if f.name not in skip]

    with open(filepath, "w", newline="") as f:
        writer = csv.writer(f)
        header = ["metric"] + [f"{p}_{stat}" for p in profiles for stat in ["mean", "median", "std"]]
        writer.writerow(header)
        for name in metric_names:
            row = [name]
            for p in profiles:
                row.extend([
                    f"{profiles[p][name]['mean']:.4f}",
                    f"{profiles[p][name]['median']:.4f}",
                    f"{profiles[p][name]['std']:.4f}",
                ])
            writer.writerow(row)


# ============================================================
# Main
# ============================================================

PLAYERS = [
    ("Karpov", "Karpov.zip", "Karpov"),
    ("Petrosian", "Petrosian.zip", "Petrosian"),
    ("Keres", "Keres.zip", "Keres"),
]


def main():
    parser = argparse.ArgumentParser(description="Karpov Game Analyzer")
    parser.add_argument("--sample-start", type=int, default=10, help="First move to sample (default 10)")
    parser.add_argument("--sample-end", type=int, default=40, help="Last move to sample (default 40)")
    parser.add_argument("--max-games", type=int, default=0, help="Max games per player (0=all)")
    args = parser.parse_args()

    all_metrics = []

    print(f"\n{BOLD}{CYAN}  Loading and analyzing games...{RESET}\n")

    for player_name, zip_file, search_name in PLAYERS:
        zip_path = os.path.join(GAMES_DIR, zip_file)
        if not os.path.exists(zip_path):
            print(f"  {RED}⚠ {zip_path} not found — skipping{RESET}")
            continue

        print(f"  {BOLD}{player_name}{RESET}: loading...", end="", flush=True)
        games = load_games(zip_path, search_name, args.max_games)
        print(f" {len(games)} games", end="", flush=True)

        t0 = time.time()
        player_metrics = []
        max_workers = min(24, len(games))
        with ProcessPoolExecutor(max_workers=max_workers) as executor:
            futures = {
                executor.submit(process_game, game, search_name, i, args.sample_start, args.sample_end): i
                for i, game in enumerate(games)
            }
            for future in as_completed(futures):
                player_metrics.extend(future.result())
                done = len([f for f in futures if futures[f] is not None])
                if done % 500 == 0:
                    print(f"\r  {BOLD}{player_name}{RESET}: {done}/{len(games)} games, {len(player_metrics)} positions", end="", flush=True)

        elapsed = time.time() - t0
        print(f"\r  {BOLD}{player_name}{RESET}: {len(games)} games → {len(player_metrics)} positions ({elapsed:.1f}s)")
        all_metrics.extend(player_metrics)

    if not all_metrics:
        print(f"{RED}No metrics collected!{RESET}")
        return

    print(f"\n  Total: {len(all_metrics)} position samples")

    # Save raw data
    csv_path = os.path.join(OUTPUT_DIR, "metrics.csv")
    save_csv(all_metrics, csv_path)
    print(f"  Saved raw metrics to {csv_path}")

    # Analyze
    profiles, win_corr, discrimination, metric_names = analyze_results(all_metrics)

    # Save profiles
    profiles_path = os.path.join(OUTPUT_DIR, "player_profiles.csv")
    save_profiles(profiles, profiles_path)
    print(f"  Saved player profiles to {profiles_path}")

    # Print report
    print_report(profiles, win_corr, discrimination, metric_names, all_metrics)


if __name__ == "__main__":
    main()
