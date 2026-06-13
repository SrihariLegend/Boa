#!/usr/bin/env python3
"""
Absurd Metric Analyzer — Unconventional chess metrics on master games.

Goes beyond standard chess metrics to find hidden patterns in how
Karpov, Petrosian, and Keres win games. Think "Moneyball for chess."

Some metrics are conventional, some are weird, some are downright absurd.
The point: let the data tell us what matters, not our preconceptions.

Usage:
    python3 tools/absurd_analyzer.py [--max-games 0]
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
from collections import defaultdict, Counter
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

PIECE_VALUES = {
    chess.PAWN: 100, chess.KNIGHT: 320, chess.BISHOP: 330,
    chess.ROOK: 500, chess.QUEEN: 900, chess.KING: 0
}


# ============================================================
# Game-level metrics (computed per game, not per position)
# ============================================================

@dataclass
class GameMetrics:
    # Identity
    player: str = ""
    game_idx: int = 0
    result: str = ""  # W/D/L
    player_color: int = 0  # 0=white, 1=black
    opponent_elo: int = 0
    year: int = 0
    eco: str = ""

    # ── Game shape metrics ──
    game_length: int = 0                # total ply
    moves_as_white: int = 0             # total moves when playing white

    # ── Timing / Tempo metrics ──
    avg_move_number_of_captures: float = 0.0  # when do captures happen on average?
    first_capture_move: int = 0          # move number of first capture
    last_capture_move: int = 0           # move number of last capture
    captures_total: int = 0              # total captures in game
    captures_by_player: int = 0          # captures made by the player
    captures_by_opponent: int = 0        # captures made by opponent
    capture_ratio: float = 0.0           # player captures / opponent captures

    # ── Piece trading patterns ──
    first_queen_trade_move: int = 0      # when did queens come off? 0 = never
    queens_traded: int = 0               # did queens get traded? 0/1
    pieces_remaining_at_move_20: int = 0 # how many pieces left at move 20?
    pieces_remaining_at_move_30: int = 0
    pieces_remaining_at_move_40: int = 0
    simplification_rate: float = 0.0     # pieces removed per move

    # ── Pawn metrics (game-wide) ──
    pawn_moves_total: int = 0            # total pawn moves by player
    pawn_move_ratio: float = 0.0         # pawn moves / total moves
    central_pawn_moves: int = 0          # d/e pawn moves
    flank_pawn_moves: int = 0            # a/b/g/h pawn moves
    pawn_pushes_past_4th: int = 0        # pawns advanced beyond 4th rank
    first_pawn_break_move: int = 0       # first pawn capture

    # ── Knight odyssey ──
    knight_moves_total: int = 0
    knight_retreats: int = 0             # knight moves backward
    knight_to_rim: int = 0               # knight to a/h file
    max_knight_journey: int = 0          # longest sequence of consecutive knight moves

    # ── Bishop behavior ──
    bishop_moves_total: int = 0
    bishop_long_diag_moves: int = 0      # moves along a1-h8 or a8-h1 diagonals
    fianchetto_played: int = 0           # g3+Bg2 or b3+Bb2 (or black equiv)

    # ── Rook behavior ──
    rook_moves_total: int = 0
    rook_lift: int = 0                   # rook to 3rd/4th rank (white) or 5th/6th (black)
    rook_to_7th: int = 0                 # rook reaches 7th rank
    first_rook_move: int = 0             # when did first rook move happen?
    rooks_connected_move: int = 0        # first move where rooks see each other on back rank

    # ── Queen behavior ──
    queen_moves_total: int = 0
    queen_moves_before_move_10: int = 0  # early queen development
    queen_centralization_moves: int = 0  # queen to d4/d5/e4/e5

    # ── King behavior ──
    king_moves_total: int = 0
    king_moves_before_castling: int = 0  # king moved before castling? (usually bad)
    castled_move: int = 0                # move number when castled (0 = never)
    castled_kingside: int = 0            # 0/1
    castled_queenside: int = 0           # 0/1
    king_walk_in_endgame: int = 0        # king moves after move 30

    # ── Check patterns ──
    checks_given: int = 0
    checks_received: int = 0
    check_ratio: float = 0.0

    # ── Move type distribution ──
    total_player_moves: int = 0
    piece_type_entropy: float = 0.0      # how evenly distributed are piece moves?
    most_moved_piece_type: int = 0       # which piece type moved most? (1-6)
    repeat_square_visits: int = 0        # pieces revisiting same square

    # ── Absurd positional metrics ──
    avg_piece_distance_from_king: float = 0.0  # how spread out are pieces from king?
    max_piece_distance_from_king: int = 0
    pieces_on_edge: float = 0.0          # avg pieces on a/h/1/8 files/ranks
    pieces_in_center: float = 0.0        # avg pieces on d4/d5/e4/e5
    piece_clustering: float = 0.0        # how tightly grouped are our pieces?

    # ── Symmetry / Asymmetry ──
    position_symmetry_at_move_10: float = 0.0   # how symmetric is the position?
    pawn_structure_asymmetry: float = 0.0       # difference in pawn structure
    material_swings: int = 0             # number of times material balance changed sign

    # ── "Boring" vs "Exciting" ──
    consecutive_non_capture_streak: int = 0  # longest streak without captures
    moves_in_same_half: int = 0          # moves where piece stays on same side of board
    piece_type_switches: int = 0         # how often do we alternate which piece we move?
    consecutive_same_piece: int = 0      # longest streak of moving the same piece type

    # ── Opening specific ──
    e4_played: int = 0                   # 1 if 1.e4
    d4_played: int = 0                   # 1 if 1.d4
    c4_played: int = 0                   # 1 if 1.c4
    nf3_played: int = 0                  # 1 if 1.Nf3
    hypermodern_opening: int = 0         # fianchetto in first 6 moves
    gambit_played: int = 0               # pawn sacrificed in first 10 moves

    # ── Board geography (absurd) ──
    avg_latitude: float = 0.0            # average rank of our pieces (higher = more advanced)
    avg_longitude: float = 0.0           # average file of our pieces (4 = centered)
    kingside_pieces: float = 0.0         # avg pieces on files e-h
    queenside_pieces: float = 0.0        # avg pieces on files a-d
    board_quadrant_entropy: float = 0.0  # how evenly spread across 4 quadrants?

    # ── Specific pawn pushes (the b4 question!) ──
    a_pawn_pushed: int = 0
    b_pawn_pushed: int = 0
    c_pawn_pushed: int = 0
    d_pawn_pushed: int = 0
    e_pawn_pushed: int = 0
    f_pawn_pushed: int = 0
    g_pawn_pushed: int = 0
    h_pawn_pushed: int = 0

    # ── Endgame patterns ──
    reached_endgame: int = 0             # NPM < 2000 at any point
    endgame_length: int = 0              # moves after NPM drops below 2000
    endgame_king_activity: float = 0.0   # king centralization in endgame positions

    # ── "Weird" composite metrics ──
    aggression_index: float = 0.0        # captures + checks + advanced pawns
    passivity_index: float = 0.0         # retreats + non-captures + pieces on home rank
    chaos_index: float = 0.0            # material swings + checks + captures
    grind_index: float = 0.0            # game length * (1 - capture_ratio) * simplification
    initiative_index: float = 0.0        # advanced space + checks given - checks received


def chebyshev(sq1, sq2):
    f1, r1 = chess.square_file(sq1), chess.square_rank(sq1)
    f2, r2 = chess.square_file(sq2), chess.square_rank(sq2)
    return max(abs(f1 - f2), abs(r1 - r2))


def entropy(counts):
    """Shannon entropy of a distribution."""
    total = sum(counts)
    if total == 0:
        return 0.0
    probs = [c / total for c in counts if c > 0]
    return -sum(p * math.log2(p) for p in probs)


def position_symmetry(board):
    """Measure how symmetric the position is (0=asymmetric, 1=perfectly symmetric)."""
    matches = 0
    total = 0
    for sq in range(32):  # only check half the board
        f = chess.square_file(sq)
        r = chess.square_rank(sq)
        mirror_sq = chess.square(f, 7 - r)
        p1 = board.piece_at(sq)
        p2 = board.piece_at(mirror_sq)
        total += 1
        if p1 is None and p2 is None:
            matches += 1
        elif p1 and p2 and p1.piece_type == p2.piece_type and p1.color != p2.color:
            matches += 1
    return matches / max(total, 1)


def compute_game_metrics(game, player_name, game_idx):
    """Compute all metrics for a single game."""
    m = GameMetrics()
    m.game_idx = game_idx

    # ── Headers ──
    white = game.headers.get("White", "")
    black = game.headers.get("Black", "")
    result_str = game.headers.get("Result", "*")
    m.eco = game.headers.get("ECO", "")

    try:
        date_str = game.headers.get("Date", "????")
        m.year = int(date_str[:4]) if date_str[:4].isdigit() else 0
    except:
        m.year = 0

    if player_name.lower() in white.lower():
        color = chess.WHITE
    elif player_name.lower() in black.lower():
        color = chess.BLACK
    else:
        return None

    m.player = player_name
    m.player_color = 0 if color == chess.WHITE else 1

    if result_str == "1-0":
        m.result = "W" if color == chess.WHITE else "L"
    elif result_str == "0-1":
        m.result = "L" if color == chess.WHITE else "W"
    elif result_str == "1/2-1/2":
        m.result = "D"
    else:
        return None

    # Get opponent elo
    opp_elo_str = game.headers.get("BlackElo" if color == chess.WHITE else "WhiteElo", "")
    try:
        m.opponent_elo = int(opp_elo_str) if opp_elo_str else 0
    except:
        m.opponent_elo = 0

    # ── Walk through all moves ──
    board = game.board()
    move_num = 0
    player_move_num = 0
    capture_moves = []
    piece_move_counts = Counter()  # piece_type -> count
    square_visits = Counter()     # square -> count
    prev_piece_type = None
    same_piece_streak = 0
    max_same_piece_streak = 0
    piece_type_switches = 0
    non_capture_streak = 0
    max_non_capture_streak = 0
    material_balance_prev = 0
    material_swings = 0
    knight_consecutive = 0
    max_knight_journey = 0
    last_moved_piece_type = None
    castled = False
    king_moved_before_castle = False
    pawn_files_pushed = set()

    # Position sampling for avg metrics
    piece_distances = []
    pieces_on_edge_samples = []
    pieces_in_center_samples = []
    piece_clustering_samples = []
    latitude_samples = []
    longitude_samples = []
    ks_pieces_samples = []
    qs_pieces_samples = []
    quadrant_entropy_samples = []
    endgame_king_centralization = []

    for node in game.mainline():
        move = node.move
        moving_piece = board.piece_at(move.from_square)
        is_player_move = board.turn == color
        is_capture = board.is_capture(move)
        gives_check = False
        move_num += 1

        # Track some things before pushing
        mat_before = sum(PIECE_VALUES.get(p.piece_type, 0) for sq, p in board.piece_map().items())

        board.push(move)

        gives_check = board.is_check()
        mat_after = sum(PIECE_VALUES.get(p.piece_type, 0) for sq, p in board.piece_map().items())

        # Material balance tracking
        our_mat = sum(PIECE_VALUES.get(p.piece_type, 0) for sq, p in board.piece_map().items() if p.color == color)
        opp_mat = sum(PIECE_VALUES.get(p.piece_type, 0) for sq, p in board.piece_map().items() if p.color != color)
        mat_bal = our_mat - opp_mat
        if material_balance_prev != 0 and mat_bal != 0:
            if (mat_bal > 0) != (material_balance_prev > 0):
                material_swings += 1
        material_balance_prev = mat_bal

        if is_player_move and moving_piece:
            player_move_num += 1
            pt = moving_piece.piece_type

            # Piece move counts
            piece_move_counts[pt] += 1
            square_visits[move.to_square] += 1

            # Piece type switching
            if last_moved_piece_type is not None:
                if pt != last_moved_piece_type:
                    piece_type_switches += 1
            if pt == last_moved_piece_type:
                same_piece_streak += 1
                max_same_piece_streak = max(max_same_piece_streak, same_piece_streak)
            else:
                same_piece_streak = 1
            last_moved_piece_type = pt

            # Captures
            if is_capture:
                m.captures_by_player += 1
                capture_moves.append(move_num)
                non_capture_streak = 0
            else:
                non_capture_streak += 1
                max_non_capture_streak = max(max_non_capture_streak, non_capture_streak)

            # Checks
            if gives_check:
                m.checks_given += 1

            # ── Piece-specific tracking ──
            if pt == chess.PAWN:
                m.pawn_moves_total += 1
                to_file = chess.square_file(move.to_square)
                to_rank = chess.square_rank(move.to_square)
                if to_file in [3, 4]:  # d, e
                    m.central_pawn_moves += 1
                if to_file in [0, 1, 6, 7]:  # a, b, g, h
                    m.flank_pawn_moves += 1
                if (color == chess.WHITE and to_rank >= 4) or (color == chess.BLACK and to_rank <= 3):
                    m.pawn_pushes_past_4th += 1
                if is_capture and m.first_pawn_break_move == 0:
                    m.first_pawn_break_move = move_num

                # Track which pawn files were pushed
                from_file = chess.square_file(move.from_square)
                pawn_files_pushed.add(from_file)

            elif pt == chess.KNIGHT:
                m.knight_moves_total += 1
                to_file = chess.square_file(move.to_square)
                to_rank = chess.square_rank(move.to_square)
                from_rank = chess.square_rank(move.from_square)
                if to_file in [0, 7]:
                    m.knight_to_rim += 1
                if (color == chess.WHITE and to_rank < from_rank) or (color == chess.BLACK and to_rank > from_rank):
                    m.knight_retreats += 1
                if prev_piece_type == chess.KNIGHT:
                    knight_consecutive += 1
                    max_knight_journey = max(max_knight_journey, knight_consecutive)
                else:
                    knight_consecutive = 1

            elif pt == chess.BISHOP:
                m.bishop_moves_total += 1
                tf = chess.square_file(move.to_square)
                tr = chess.square_rank(move.to_square)
                if tf == tr or tf + tr == 7:
                    m.bishop_long_diag_moves += 1

            elif pt == chess.ROOK:
                m.rook_moves_total += 1
                tr = chess.square_rank(move.to_square)
                if m.first_rook_move == 0:
                    m.first_rook_move = move_num
                if (color == chess.WHITE and tr in [2, 3]) or (color == chess.BLACK and tr in [4, 5]):
                    m.rook_lift += 1
                if (color == chess.WHITE and tr == 6) or (color == chess.BLACK and tr == 1):
                    m.rook_to_7th += 1

            elif pt == chess.QUEEN:
                m.queen_moves_total += 1
                if move_num <= 20:  # first 10 moves (counting both sides)
                    m.queen_moves_before_move_10 += 1
                if move.to_square in [chess.D4, chess.D5, chess.E4, chess.E5]:
                    m.queen_centralization_moves += 1

            elif pt == chess.KING:
                m.king_moves_total += 1
                if not castled:
                    # Check if this IS a castling move
                    from_f = chess.square_file(move.from_square)
                    to_f = chess.square_file(move.to_square)
                    if abs(from_f - to_f) >= 2 and from_f == 4:
                        castled = True
                        m.castled_move = (move_num + 1) // 2
                        if to_f > 4:
                            m.castled_kingside = 1
                        else:
                            m.castled_queenside = 1
                    else:
                        m.king_moves_before_castling += 1

                if move_num > 60:  # after move 30
                    m.king_walk_in_endgame += 1

            prev_piece_type = pt

        elif not is_player_move:
            if is_capture:
                m.captures_by_opponent += 1
                capture_moves.append(move_num)
            if gives_check:
                m.checks_received += 1

        # ── Position sampling (every 4 ply for efficiency) ──
        if move_num % 4 == 0 and move_num >= 10:
            our_pieces = [(sq, p) for sq, p in board.piece_map().items() if p.color == color]
            king_sq = board.king(color)

            if king_sq is not None and our_pieces:
                # Distance from king
                dists = [chebyshev(sq, king_sq) for sq, p in our_pieces if sq != king_sq]
                if dists:
                    piece_distances.append(sum(dists) / len(dists))

                # Pieces on edge
                edge = sum(1 for sq, p in our_pieces
                           if chess.square_file(sq) in [0, 7] or chess.square_rank(sq) in [0, 7])
                pieces_on_edge_samples.append(edge)

                # Pieces in center
                center = sum(1 for sq, p in our_pieces
                             if sq in [chess.D4, chess.D5, chess.E4, chess.E5])
                pieces_in_center_samples.append(center)

                # Clustering: average distance between our pieces
                if len(our_pieces) >= 2:
                    total_dist = 0
                    count = 0
                    for i in range(len(our_pieces)):
                        for j in range(i + 1, len(our_pieces)):
                            total_dist += chebyshev(our_pieces[i][0], our_pieces[j][0])
                            count += 1
                    piece_clustering_samples.append(total_dist / count if count else 0)

                # Latitude/longitude
                ranks = [chess.square_rank(sq) for sq, p in our_pieces]
                files = [chess.square_file(sq) for sq, p in our_pieces]
                latitude_samples.append(sum(ranks) / len(ranks))
                longitude_samples.append(sum(files) / len(files))

                # Kingside vs queenside
                ks = sum(1 for sq, p in our_pieces if chess.square_file(sq) >= 4)
                qs = sum(1 for sq, p in our_pieces if chess.square_file(sq) < 4)
                ks_pieces_samples.append(ks)
                qs_pieces_samples.append(qs)

                # Quadrant entropy
                q = [0, 0, 0, 0]
                for sq, p in our_pieces:
                    f, r = chess.square_file(sq), chess.square_rank(sq)
                    qi = (1 if f >= 4 else 0) + (2 if r >= 4 else 0)
                    q[qi] += 1
                quadrant_entropy_samples.append(entropy(q))

            # Endgame detection
            total_npm = sum(PIECE_VALUES.get(p.piece_type, 0) for sq, p in board.piece_map().items()
                           if p.piece_type != chess.PAWN)
            if total_npm < 2000:
                if m.reached_endgame == 0:
                    m.reached_endgame = 1
                m.endgame_length += 1
                if king_sq is not None:
                    kf = chess.square_file(king_sq)
                    kr = chess.square_rank(king_sq)
                    centralization = 7 - (abs(3 - kf) + abs(3 - kr))
                    endgame_king_centralization.append(centralization)

        # ── Pieces remaining at milestones ──
        if move_num == 40:
            m.pieces_remaining_at_move_20 = len(board.piece_map())
        if move_num == 60:
            m.pieces_remaining_at_move_30 = len(board.piece_map())
        if move_num == 80:
            m.pieces_remaining_at_move_40 = len(board.piece_map())

        # Position symmetry at move 10
        if move_num == 20:
            m.position_symmetry_at_move_10 = position_symmetry(board)

    # ── Post-game calculations ──
    m.game_length = move_num
    m.total_player_moves = player_move_num
    m.captures_total = len(capture_moves)
    m.consecutive_non_capture_streak = max_non_capture_streak
    m.consecutive_same_piece = max_same_piece_streak
    m.piece_type_switches = piece_type_switches
    m.material_swings = material_swings
    m.max_knight_journey = max_knight_journey
    m.repeat_square_visits = sum(1 for sq, cnt in square_visits.items() if cnt > 1)

    # Capture timing
    if capture_moves:
        m.avg_move_number_of_captures = sum(capture_moves) / len(capture_moves)
        m.first_capture_move = capture_moves[0]
        m.last_capture_move = capture_moves[-1]

    m.capture_ratio = m.captures_by_player / max(m.captures_by_opponent, 1)

    # Pawn move ratio
    m.pawn_move_ratio = m.pawn_moves_total / max(player_move_num, 1)

    # Check ratio
    m.check_ratio = m.checks_given / max(m.checks_received, 1)

    # Piece type entropy
    pt_counts = [piece_move_counts.get(pt, 0) for pt in range(1, 7)]
    m.piece_type_entropy = entropy(pt_counts)
    if pt_counts:
        m.most_moved_piece_type = pt_counts.index(max(pt_counts)) + 1

    # Simplification rate
    if m.game_length > 0:
        initial_pieces = 32
        final_pieces = m.pieces_remaining_at_move_40 if m.pieces_remaining_at_move_40 > 0 else 20
        m.simplification_rate = (initial_pieces - final_pieces) / max(m.game_length, 1)

    # Queen trade detection
    if board.pieces(chess.QUEEN, chess.WHITE).issubset(chess.SquareSet()) and \
       board.pieces(chess.QUEEN, chess.BLACK).issubset(chess.SquareSet()):
        m.queens_traded = 1

    # Fianchetto detection (rough)
    m.fianchetto_played = 1 if any(
        board.piece_at(sq) and board.piece_at(sq).piece_type == chess.BISHOP and board.piece_at(sq).color == color
        for sq in ([chess.G2, chess.B2] if color == chess.WHITE else [chess.G7, chess.B7])
    ) else 0

    # Opening moves
    try:
        first_move = list(game.mainline_moves())[0] if game.mainline_moves() else None
        if first_move and color == chess.WHITE:
            if first_move == chess.Move.from_uci("e2e4"):
                m.e4_played = 1
            elif first_move == chess.Move.from_uci("d2d4"):
                m.d4_played = 1
            elif first_move == chess.Move.from_uci("c2c4"):
                m.c4_played = 1
            elif first_move == chess.Move.from_uci("g1f3"):
                m.nf3_played = 1
    except:
        pass

    # Pawn files pushed
    for f in pawn_files_pushed:
        attr = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'][f] + '_pawn_pushed'
        setattr(m, attr, 1)

    # Pawn structure asymmetry
    w_pawns = set(chess.square_file(sq) for sq in board.pieces(chess.PAWN, chess.WHITE))
    b_pawns = set(chess.square_file(sq) for sq in board.pieces(chess.PAWN, chess.BLACK))
    m.pawn_structure_asymmetry = len(w_pawns.symmetric_difference(b_pawns))

    # Avg positional samples
    m.avg_piece_distance_from_king = sum(piece_distances) / max(len(piece_distances), 1)
    m.max_piece_distance_from_king = int(max(piece_distances)) if piece_distances else 0
    m.pieces_on_edge = sum(pieces_on_edge_samples) / max(len(pieces_on_edge_samples), 1)
    m.pieces_in_center = sum(pieces_in_center_samples) / max(len(pieces_in_center_samples), 1)
    m.piece_clustering = sum(piece_clustering_samples) / max(len(piece_clustering_samples), 1)
    m.avg_latitude = sum(latitude_samples) / max(len(latitude_samples), 1)
    m.avg_longitude = sum(longitude_samples) / max(len(longitude_samples), 1)
    m.kingside_pieces = sum(ks_pieces_samples) / max(len(ks_pieces_samples), 1)
    m.queenside_pieces = sum(qs_pieces_samples) / max(len(qs_pieces_samples), 1)
    m.board_quadrant_entropy = sum(quadrant_entropy_samples) / max(len(quadrant_entropy_samples), 1)
    m.endgame_king_activity = sum(endgame_king_centralization) / max(len(endgame_king_centralization), 1)

    # ── Composite "absurd" metrics ──
    m.aggression_index = (m.captures_by_player + m.checks_given * 2 + m.pawn_pushes_past_4th) / max(player_move_num, 1) * 100
    m.passivity_index = (m.knight_retreats + m.consecutive_non_capture_streak + m.flank_pawn_moves) / max(player_move_num, 1) * 100
    m.chaos_index = (m.material_swings * 10 + m.checks_given + m.checks_received + m.captures_total) / max(m.game_length, 1) * 100
    m.grind_index = m.game_length * (1.0 / max(m.capture_ratio, 0.1)) * m.simplification_rate
    m.initiative_index = (m.pawn_pushes_past_4th * 2 + m.checks_given * 3 - m.checks_received * 2 + m.rook_to_7th * 5) / max(player_move_num, 1) * 100

    return m


def load_games(zip_path, max_games=0):
    """Load games from a zip file."""
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

def analyze_metrics(all_metrics):
    """Perform statistical analysis."""
    by_player = defaultdict(list)
    for m in all_metrics:
        by_player[m.player].append(m)

    skip = {"player", "game_idx", "result", "player_color", "opponent_elo", "year", "eco",
            "most_moved_piece_type"}
    metric_names = [f.name for f in fields(GameMetrics) if f.name not in skip and f.type in (int, float)]

    # Player profiles
    profiles = {}
    for player, data in by_player.items():
        profile = {}
        for name in metric_names:
            values = [getattr(m, name) for m in data]
            if values:
                mean = sum(values) / len(values)
                std = math.sqrt(sum((v - mean) ** 2 for v in values) / max(len(values) - 1, 1))
                profile[name] = {"mean": mean, "median": sorted(values)[len(values) // 2], "std": std}
            else:
                profile[name] = {"mean": 0, "median": 0, "std": 0}
        profiles[player] = profile

    # Win correlation
    win_corr = {}
    for name in metric_names:
        wins = [getattr(m, name) for m in all_metrics if m.result == "W"]
        losses = [getattr(m, name) for m in all_metrics if m.result == "L"]
        draws = [getattr(m, name) for m in all_metrics if m.result == "D"]

        win_mean = sum(wins) / max(len(wins), 1)
        loss_mean = sum(losses) / max(len(losses), 1)
        draw_mean = sum(draws) / max(len(draws), 1)

        all_vals = wins + losses
        if len(all_vals) > 1:
            overall_mean = sum(all_vals) / len(all_vals)
            pooled_std = math.sqrt(sum((v - overall_mean) ** 2 for v in all_vals) / (len(all_vals) - 1))
        else:
            pooled_std = 1

        effect_size = (win_mean - loss_mean) / max(pooled_std, 0.01)

        if wins and losses:
            count_greater = sum(1 for w in wins[:500] for l in losses[:500] if w > l)
            total_pairs = min(len(wins), 500) * min(len(losses), 500)
            auc = count_greater / max(total_pairs, 1)
        else:
            auc = 0.5

        win_corr[name] = {
            "win_mean": win_mean, "loss_mean": loss_mean, "draw_mean": draw_mean,
            "effect_size": effect_size, "auc": auc,
            "n_wins": len(wins), "n_losses": len(losses), "n_draws": len(draws),
        }

    # Player discrimination
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
                pooled = math.sqrt((std1 ** 2 + std2 ** 2) / 2) if (std1 + std2) > 0 else 1
                d = abs(p1 - p2) / max(pooled, 0.01)
                max_diff = max(max_diff, d)
        discrimination[name] = max_diff

    return profiles, win_corr, discrimination, metric_names


def print_report(profiles, win_corr, discrimination, metric_names, all_metrics):
    """Print formatted report."""
    players = list(profiles.keys())

    print()
    print(f"{BOLD}{'═' * 100}{RESET}")
    print(f"{BOLD}{CYAN}  Absurd Metric Analyzer — Unconventional Chess Game Analysis{RESET}")
    print(f"{BOLD}{'═' * 100}{RESET}")

    # Game counts
    by_player = defaultdict(lambda: {"W": 0, "D": 0, "L": 0, "total": 0})
    for m in all_metrics:
        by_player[m.player]["total"] += 1
        by_player[m.player][m.result] += 1

    for p in players:
        s = by_player[p]
        print(f"  {BOLD}{p}{RESET}: {s['total']} games ({s['W']}W {s['D']}D {s['L']}L)")

    # ── Top Win Predictors ──
    print(f"\n{BOLD}{'─' * 100}{RESET}")
    print(f"{BOLD}{CYAN}  Top 30 Metrics by Win Prediction{RESET}")
    print(f"{BOLD}{'─' * 100}{RESET}")
    print(f"  {'Metric':<35} {'Effect':>7} {'AUC':>5}  {'Win μ':>10} {'Loss μ':>10} {'Draw μ':>10}")
    print(f"  {'─' * 90}")

    sorted_win = sorted(metric_names, key=lambda n: abs(win_corr[n]["effect_size"]), reverse=True)
    for name in sorted_win[:30]:
        wc = win_corr[name]
        eff = wc["effect_size"]
        color = GREEN if eff > 0.1 else RED if eff < -0.1 else ""
        reset = RESET if color else ""
        print(f"  {color}{name:<35} {eff:>+7.3f} {wc['auc']:>5.3f}  {wc['win_mean']:>10.2f} {wc['loss_mean']:>10.2f} {wc['draw_mean']:>10.2f}{reset}")

    # ── Top Style Discriminators ──
    print(f"\n{BOLD}{'─' * 100}{RESET}")
    print(f"{BOLD}{CYAN}  Top 30 Metrics by Player Style Discrimination{RESET}")
    print(f"{BOLD}{'─' * 100}{RESET}")
    print(f"  {'Metric':<35} {'Max d':>7}  ", end="")
    for p in players:
        print(f"{p[:10]:>12}", end="")
    print()
    print(f"  {'─' * 90}")

    sorted_disc = sorted(metric_names, key=lambda n: discrimination[n], reverse=True)
    for name in sorted_disc[:30]:
        d = discrimination[name]
        print(f"  {name:<35} {d:>7.3f}  ", end="")
        for p in players:
            val = profiles[p][name]["mean"]
            print(f"{val:>12.2f}", end="")
        print()

    # ── Combined ranking ──
    print(f"\n{BOLD}{'─' * 100}{RESET}")
    print(f"{BOLD}{CYAN}  Combined Metric Ranking (Top 40){RESET}")
    print(f"{BOLD}{'─' * 100}{RESET}")

    max_eff = max(abs(win_corr[n]["effect_size"]) for n in metric_names) or 1
    max_disc = max(discrimination[n] for n in metric_names) or 1

    combined = {}
    for name in metric_names:
        eff_norm = abs(win_corr[name]["effect_size"]) / max_eff
        disc_norm = discrimination[name] / max_disc
        auc_norm = abs(win_corr[name]["auc"] - 0.5) * 2
        combined[name] = 0.4 * eff_norm + 0.3 * disc_norm + 0.3 * auc_norm

    sorted_combined = sorted(metric_names, key=lambda n: combined[n], reverse=True)
    print(f"  {'Rank':<5} {'Metric':<35} {'Score':>7} {'WinEff':>7} {'Discr':>7} {'AUC':>7}")
    print(f"  {'─' * 75}")
    for i, name in enumerate(sorted_combined[:40]):
        sc = combined[name]
        wc = win_corr[name]
        dc = discrimination[name]
        color = GREEN if sc > 0.5 else YELLOW if sc > 0.3 else ""
        reset = RESET if color else ""
        print(f"  {color}{i+1:<5} {name:<35} {sc:>7.3f} {wc['effect_size']:>+7.3f} {dc:>7.3f} {wc['auc']:>7.3f}{reset}")

    # ── Absurd findings ──
    print(f"\n{BOLD}{'─' * 100}{RESET}")
    print(f"{BOLD}{CYAN}  🎯 Most Absurd / Surprising Findings{RESET}")
    print(f"{BOLD}{'─' * 100}{RESET}")

    # Find the most surprising metrics: high win prediction but you wouldn't expect it
    absurd_candidates = [
        "b_pawn_pushed", "h_pawn_pushed", "a_pawn_pushed", "f_pawn_pushed", "g_pawn_pushed",
        "knight_to_rim", "knight_retreats", "queen_moves_before_move_10",
        "consecutive_same_piece", "piece_type_entropy", "avg_latitude", "avg_longitude",
        "board_quadrant_entropy", "pieces_on_edge", "piece_clustering",
        "fianchetto_played", "aggression_index", "passivity_index", "chaos_index",
        "grind_index", "initiative_index", "rook_lift", "max_knight_journey",
        "bishop_long_diag_moves", "position_symmetry_at_move_10",
        "king_walk_in_endgame", "avg_piece_distance_from_king",
        "consecutive_non_capture_streak", "capture_ratio", "check_ratio",
        "pawn_move_ratio", "simplification_rate",
    ]

    absurd_found = [(n, win_corr[n], discrimination[n]) for n in absurd_candidates if n in win_corr]
    absurd_found.sort(key=lambda x: abs(x[1]["effect_size"]), reverse=True)

    for name, wc, disc in absurd_found[:20]:
        eff = wc["effect_size"]
        direction = "↑ wins" if eff > 0 else "↓ wins"
        print(f"\n  {BOLD}{name}{RESET}: effect={eff:+.3f} ({direction}), discrimination={disc:.3f}")
        for p in players:
            val = profiles[p][name]["mean"]
            print(f"    {p[:12]:<12} {val:>10.3f}")

    # ── Player personas ──
    print(f"\n{BOLD}{'─' * 100}{RESET}")
    print(f"{BOLD}{CYAN}  🎭 Player Personas (unique traits vs other players){RESET}")
    print(f"{BOLD}{'─' * 100}{RESET}")

    for player in players:
        print(f"\n  {BOLD}{CYAN}{player}{RESET}:")
        traits = []
        for name in metric_names:
            val = profiles[player][name]["mean"]
            others = [profiles[p][name]["mean"] for p in players if p != player]
            avg_others = sum(others) / max(len(others), 1)
            if avg_others == 0 and val == 0:
                continue
            std = profiles[player][name]["std"]
            if std > 0:
                z = (val - avg_others) / std
                if abs(z) > 0.3:
                    traits.append((name, val, avg_others, z))

        traits.sort(key=lambda x: abs(x[3]), reverse=True)
        for name, val, avg_o, z in traits[:10]:
            direction = "higher" if z > 0 else "lower"
            bar = "█" * min(int(abs(z) * 20), 40)
            color = GREEN if z > 0 else RED
            print(f"    {color}{name:<35} {val:>8.2f} (others: {avg_o:.2f}) {direction} {bar}{RESET}")

    print(f"\n{BOLD}{'═' * 100}{RESET}")


def save_csv(all_metrics, filepath):
    """Save all metrics to CSV."""
    os.makedirs(os.path.dirname(filepath), exist_ok=True)
    field_names = [f.name for f in fields(GameMetrics)]
    with open(filepath, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=field_names)
        writer.writeheader()
        for m in all_metrics:
            writer.writerow(asdict(m))


# ============================================================
# Main
# ============================================================

PLAYERS = [
    ("Karpov", "Karpov.zip", "Karpov"),
    ("Petrosian", "Petrosian.zip", "Petrosian"),
    ("Keres", "Keres.zip", "Keres"),
]


def main():
    parser = argparse.ArgumentParser(description="Absurd Metric Analyzer")
    parser.add_argument("--max-games", type=int, default=0, help="Max games per player (0=all)")
    args = parser.parse_args()

    all_metrics = []

    print(f"\n{BOLD}{CYAN}  Loading and analyzing games (absurd metrics)...{RESET}\n")

    for player_name, zip_file, search_name in PLAYERS:
        zip_path = os.path.join(GAMES_DIR, zip_file)
        if not os.path.exists(zip_path):
            print(f"  {RED}⚠ {zip_path} not found — skipping{RESET}")
            continue

        print(f"  {BOLD}{player_name}{RESET}: loading...", end="", flush=True)
        games = load_games(zip_path, args.max_games)
        print(f" {len(games)} games", end="", flush=True)

        t0 = time.time()
        player_metrics = []
        max_workers = min(24, len(games))
        with ProcessPoolExecutor(max_workers=max_workers) as executor:
            futures = {
                executor.submit(compute_game_metrics, game, search_name, i): i
                for i, game in enumerate(games)
            }
            for future in as_completed(futures):
                result = future.result()
                if result:
                    player_metrics.append(result)
                done = len([f for f in futures if futures[f] is not None])
                if done % 500 == 0:
                    print(f"\r  {BOLD}{player_name}{RESET}: {done}/{len(games)} games processed", end="", flush=True)

        elapsed = time.time() - t0
        print(f"\r  {BOLD}{player_name}{RESET}: {len(games)} games → {len(player_metrics)} analyzed ({elapsed:.1f}s)")
        all_metrics.extend(player_metrics)

    if not all_metrics:
        print(f"{RED}No metrics collected!{RESET}")
        return

    print(f"\n  Total: {len(all_metrics)} games analyzed")

    # Save
    csv_path = os.path.join(OUTPUT_DIR, "absurd_metrics.csv")
    save_csv(all_metrics, csv_path)
    print(f"  Saved to {csv_path}")

    # Analyze
    profiles, win_corr, discrimination, metric_names = analyze_metrics(all_metrics)

    # Save profiles
    profiles_path = os.path.join(OUTPUT_DIR, "absurd_profiles.csv")
    os.makedirs(os.path.dirname(profiles_path), exist_ok=True)
    skip_fields = {"player", "game_idx", "result", "player_color", "opponent_elo", "year", "eco", "most_moved_piece_type"}
    with open(profiles_path, "w", newline="") as f:
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
    print(f"  Saved profiles to {profiles_path}")

    # Report
    print_report(profiles, win_corr, discrimination, metric_names, all_metrics)

    # Save report
    report_path = os.path.join(OUTPUT_DIR, "absurd_report.txt")
    import contextlib
    with open(report_path, "w") as f:
        with contextlib.redirect_stdout(f):
            print_report(profiles, win_corr, discrimination, metric_names, all_metrics)
    print(f"\n  Report saved to {report_path}")


if __name__ == "__main__":
    main()
