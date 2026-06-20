#!/usr/bin/env python3
"""Tune Boa's internal eval weights from a self-play Texel CSV.

This tuner is intentionally separate from tools/texel_tune.py. The scale tuner
fits coarse UCI component multipliers; this script decomposes the current eval
into per-constant coefficients for mobility, pawn structure, king safety,
and closely related positional terms. Material, PST, and the removed flavor
terms are left fixed.
"""

from __future__ import annotations

import argparse
import csv
import math
from dataclasses import dataclass


FILES = [0x0101010101010101 << f for f in range(8)]
RANKS = [0xFF << (8 * r) for r in range(8)]
FILE_A = FILES[0]
FILE_H = FILES[7]
PIECE_KIND = {"P": "pawn", "N": "knight", "B": "bishop", "R": "rook", "Q": "queen", "K": "king"}


@dataclass(frozen=True)
class Param:
    name: str
    value: int
    min_value: int
    max_value: int
    group: str


@dataclass
class Position:
    white: dict[str, int]
    black: dict[str, int]
    occ_white: int
    occ_black: int
    occ: int
    white_king: int | None
    black_king: int | None


@dataclass
class Dataset:
    labels: list[float]
    evals: list[float]
    coeffs: list[dict[int, float]]
    rebuild_errors: list[float]
    train_rows: list[int]
    validation_rows: list[int]


def p(name: str, value: int, lo: int, hi: int, group: str) -> Param:
    return Param(name, value, lo, hi, group)


PARAMS: list[Param] = []
PARAM_INDEX: dict[str, int] = {}


def add_param(name: str, value: int, lo: int, hi: int, group: str) -> None:
    PARAM_INDEX[name] = len(PARAMS)
    PARAMS.append(p(name, value, lo, hi, group))


def add_pair(name: str, mg: int, eg: int, lo: int, hi: int, group: str) -> None:
    add_param(f"{name}.mg", mg, lo, hi, group)
    add_param(f"{name}.eg", eg, lo, hi, group)


add_pair("BISHOP_PAIR_BONUS", 25, 80, 0, 180, "mobility")
add_pair("ROOK_OPEN_FILE_BONUS", 25, 5, 0, 120, "mobility")
add_pair("ROOK_SEMI_OPEN_FILE_BONUS", 19, 8, 0, 100, "mobility")
add_pair("ROOK_ON_SEVENTH_BONUS", 50, 35, 0, 150, "mobility")
add_param("OUTPOST_SUPPORTED", 10, 0, 100, "mobility")
add_param("OUTPOST_UNSUPPORTED", 5, 0, 80, "mobility")

for table, values in {
    "KNIGHT_MOBILITY": [(-30, -20), (-15, -10), (0, 0), (5, 5), (10, 10), (15, 15), (20, 18), (25, 20), (28, 22)],
    "BISHOP_MOBILITY": [(-30, -25), (-15, -12), (0, 0), (5, 4), (8, 7), (11, 10), (14, 13), (17, 16), (19, 18), (21, 20), (23, 22), (25, 24), (26, 25), (27, 26)],
    "ROOK_MOBILITY": [(-25, -20), (-12, -10), (0, 0), (3, 3), (5, 5), (7, 7), (9, 9), (11, 11), (13, 13), (15, 15), (17, 17), (19, 19), (20, 20), (21, 21), (22, 22)],
    "QUEEN_MOBILITY": [(-15, -10), (-8, -5), (0, 0), (2, 2), (4, 4), (6, 6), (8, 8), (10, 10), (12, 12), (13, 13), (14, 14), (15, 15), (16, 16), (17, 17), (18, 18), (19, 19), (20, 20), (21, 21), (21, 21), (22, 22), (22, 22), (23, 23), (23, 23), (24, 24), (24, 24), (24, 24), (25, 25), (25, 25)],
}.items():
    for i, (mg, eg) in enumerate(values):
        add_pair(f"{table}[{i}]", mg, eg, -100, 120, "mobility")

add_pair("DOUBLED_PAWN_PENALTY", -13, 0, -120, 40, "pawn")
add_pair("ISOLATED_PAWN_PENALTY", -15, -40, -140, 30, "pawn")
add_pair("BACKWARD_PAWN_PENALTY", -23, -30, -140, 30, "pawn")
add_pair("PAWN_CHAIN_BONUS", 6, 15, -20, 80, "pawn")
for i, value in enumerate([0, 2, 0, 5, 15, 75, 70, 0]):
    add_param(f"PASSED_PAWN_BONUS_MG[{i}]", value, -30, 220, "pawn")
for i, value in enumerate([0, 10, 5, 20, 65, 95, 115, 0]):
    add_param(f"PASSED_PAWN_BONUS_EG[{i}]", value, -30, 260, "pawn")
add_pair("ROOK_BEHIND_PASSER_BONUS", 5, 10, -30, 100, "pawn")
add_pair("CONNECTED_PASSER_BONUS", 0, 5, -30, 100, "pawn")
add_pair("PASSER_PATH_CLEAR_BONUS", 2, 15, -30, 120, "pawn")
add_param("PASSER_KING_PROXIMITY_EG", 15, -20, 100, "pawn")
add_param("PASSER_ENEMY_KING_DIST_EG", 10, -20, 100, "pawn")

add_param("PAWN_SHIELD_PER_PAWN", 22, 0, 80, "king")
add_param("PAWN_SHIELD_BASE_PENALTY", 30, 0, 140, "king")
for name, value in [
    ("KING_ATTACK_WEIGHT_KNIGHT", 2),
    ("KING_ATTACK_WEIGHT_BISHOP", 3),
    ("KING_ATTACK_WEIGHT_ROOK", 1),
    ("KING_ATTACK_WEIGHT_QUEEN", 5),
]:
    add_param(name, value, 0, 12, "king_weight")
for i, value in enumerate([0, 20, 50, 80, 120, 170, 230]):
    add_param(f"KING_SAFETY_TABLE[{i}]", value, 0, 420, "king")
add_param("KING_CENTRALIZATION_EG", 15, -30, 100, "king")

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("csv_path", nargs="?", default="analysis/self_play/texel_self_play.csv")
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--k", type=float, default=0.004)
    parser.add_argument("--steps", default="8,4,2,1")
    parser.add_argument("--passes", type=int, default=2)
    parser.add_argument(
        "--l2",
        type=float,
        default=1e-6,
        help="L2 prior strength around current constants. Default: 1e-6.",
    )
    parser.add_argument(
        "--validation-fraction",
        type=float,
        default=0.2,
        help="Deterministic holdout fraction. Default: 0.2.",
    )
    parser.add_argument(
        "--max-delta",
        type=int,
        default=12,
        help="Maximum absolute change from the current constant. Default: 12 cp.",
    )
    parser.add_argument(
        "--no-constraints",
        action="store_true",
        help="Disable semantic sign and monotonic projection constraints.",
    )
    parser.add_argument(
        "--no-validation-gate",
        action="store_true",
        help="Allow updates that improve train loss even if holdout MSE worsens.",
    )
    parser.add_argument(
        "--groups",
        default="mobility,pawn,king",
        help="Comma-separated groups to tune. Use all to include king attack weights.",
    )
    parser.add_argument(
        "--target",
        choices=["result", "future"],
        default="result",
        help="result tunes Texel game outcomes; future fits future_white_score_cp by MSE.",
    )
    return parser.parse_args()


def bb(sq: int) -> int:
    return 1 << sq


def pop_lsb(bits: int) -> tuple[int, int]:
    lsb = bits & -bits
    return lsb.bit_length() - 1, bits ^ lsb


def sign_for(color: str) -> int:
    return 1 if color == "white" else -1


def rank_of(sq: int) -> int:
    return sq // 8


def file_of(sq: int) -> int:
    return sq % 8


def orient_square(sq: int, color: str) -> int:
    if color == "white":
        return sq
    return (7 - rank_of(sq)) * 8 + file_of(sq)


def chebyshev(a: int, b: int) -> int:
    return max(abs(file_of(a) - file_of(b)), abs(rank_of(a) - rank_of(b)))


def pawn_attacks_white(pawns: int) -> int:
    return ((pawns << 9) & ~FILE_A) | ((pawns << 7) & ~FILE_H)


def pawn_attacks_black(pawns: int) -> int:
    return ((pawns >> 7) & ~FILE_A) | ((pawns >> 9) & ~FILE_H)


def knight_attacks(sq: int) -> int:
    f = file_of(sq)
    r = rank_of(sq)
    out = 0
    for df, dr in ((1, 2), (2, 1), (2, -1), (1, -2), (-1, -2), (-2, -1), (-2, 1), (-1, 2)):
        nf, nr = f + df, r + dr
        if 0 <= nf < 8 and 0 <= nr < 8:
            out |= bb(nr * 8 + nf)
    return out


def king_attacks(sq: int) -> int:
    f = file_of(sq)
    r = rank_of(sq)
    out = 0
    for df in (-1, 0, 1):
        for dr in (-1, 0, 1):
            if df == 0 and dr == 0:
                continue
            nf, nr = f + df, r + dr
            if 0 <= nf < 8 and 0 <= nr < 8:
                out |= bb(nr * 8 + nf)
    return out


def slider_attacks(sq: int, occ: int, directions: tuple[tuple[int, int], ...]) -> int:
    out = 0
    f = file_of(sq)
    r = rank_of(sq)
    for df, dr in directions:
        nf, nr = f + df, r + dr
        while 0 <= nf < 8 and 0 <= nr < 8:
            nsq = nr * 8 + nf
            out |= bb(nsq)
            if occ & bb(nsq):
                break
            nf += df
            nr += dr
    return out


def bishop_attacks(sq: int, occ: int) -> int:
    return slider_attacks(sq, occ, ((1, 1), (1, -1), (-1, 1), (-1, -1)))


def rook_attacks(sq: int, occ: int) -> int:
    return slider_attacks(sq, occ, ((1, 0), (-1, 0), (0, 1), (0, -1)))


def queen_attacks(sq: int, occ: int) -> int:
    return bishop_attacks(sq, occ) | rook_attacks(sq, occ)


def parse_fen(fen: str) -> Position:
    white = {name: 0 for name in PIECE_KIND.values()}
    black = {name: 0 for name in PIECE_KIND.values()}
    rank = 7
    file = 0
    for ch in fen.split()[0]:
        if ch == "/":
            rank -= 1
            file = 0
            continue
        if ch.isdigit():
            file += int(ch)
            continue
        target = white if ch.isupper() else black
        target[PIECE_KIND[ch.upper()]] |= bb(rank * 8 + file)
        file += 1
    occ_white = 0
    occ_black = 0
    for bits in white.values():
        occ_white |= bits
    for bits in black.values():
        occ_black |= bits
    wk = (white["king"].bit_length() - 1) if white["king"] else None
    bk = (black["king"].bit_length() - 1) if black["king"] else None
    return Position(white, black, occ_white, occ_black, occ_white | occ_black, wk, bk)


def phase_weight(phase: int) -> tuple[float, float]:
    return phase / 256.0, (256 - phase) / 256.0


def add(coeffs: dict[int, float], name: str, value: float) -> None:
    if value == 0:
        return
    idx = PARAM_INDEX[name]
    coeffs[idx] = coeffs.get(idx, 0.0) + value


def add_pair_coeff(coeffs: dict[int, float], name: str, sign: int, phase: int, count: float = 1.0) -> None:
    mgw, egw = phase_weight(phase)
    add(coeffs, f"{name}.mg", sign * count * mgw)
    add(coeffs, f"{name}.eg", sign * count * egw)


def ranks_ahead(color: str, rank: int, file_mask: int) -> int:
    out = 0
    ranks = range(rank + 1, 8) if color == "white" else range(0, rank)
    for r in ranks:
        out |= RANKS[r]
    return out & file_mask


def ranks_behind_inclusive(color: str, rank: int, file_mask: int) -> int:
    out = 0
    ranks = range(0, rank + 1) if color == "white" else range(rank, 8)
    for r in ranks:
        out |= RANKS[r]
    return out & file_mask


def side_mobility(pos: Position, color: str) -> int:
    us = pos.white if color == "white" else pos.black
    them_occ = pos.occ_black if color == "white" else pos.occ_white
    our_occ = pos.occ_white if color == "white" else pos.occ_black
    mobility = 0
    pawns = us["pawn"]
    if color == "white":
        mobility += ((pawns << 8) & ~pos.occ).bit_count()
        mobility += ((((pawns << 8) & ~pos.occ & RANKS[2]) << 8) & ~pos.occ).bit_count()
        mobility += ((pawns << 9) & ~FILE_A & them_occ).bit_count()
        mobility += ((pawns << 7) & ~FILE_H & them_occ).bit_count()
    else:
        mobility += ((pawns >> 8) & ~pos.occ).bit_count()
        mobility += ((((pawns >> 8) & ~pos.occ & RANKS[5]) >> 8) & ~pos.occ).bit_count()
        mobility += ((pawns >> 7) & ~FILE_A & them_occ).bit_count()
        mobility += ((pawns >> 9) & ~FILE_H & them_occ).bit_count()
    for piece, attack_fn in (
        ("knight", lambda sq: knight_attacks(sq)),
        ("bishop", lambda sq: bishop_attacks(sq, pos.occ)),
        ("rook", lambda sq: rook_attacks(sq, pos.occ)),
        ("queen", lambda sq: queen_attacks(sq, pos.occ)),
    ):
        bits = us[piece]
        while bits:
            sq, bits = pop_lsb(bits)
            mobility += (attack_fn(sq) & ~our_occ).bit_count()
    king = pos.white_king if color == "white" else pos.black_king
    if king is not None:
        mobility += (king_attacks(king) & ~our_occ).bit_count()
    return mobility


def mobility_coeffs(pos: Position, coeffs: dict[int, float], phase: int) -> None:
    for color in ("white", "black"):
        sign = sign_for(color)
        us = pos.white if color == "white" else pos.black
        them = pos.black if color == "white" else pos.white
        our_occ = pos.occ_white if color == "white" else pos.occ_black
        their_pawn_attacks = pawn_attacks_black(them["pawn"]) if color == "white" else pawn_attacks_white(them["pawn"])

        bits = us["knight"]
        while bits:
            sq, bits = pop_lsb(bits)
            mob = min(8, (knight_attacks(sq) & ~our_occ & ~their_pawn_attacks).bit_count())
            add_pair_coeff(coeffs, f"KNIGHT_MOBILITY[{mob}]", sign, phase)
            if not (bb(sq) & their_pawn_attacks):
                r = rank_of(sq)
                in_zone = (3 <= r <= 5) if color == "white" else (2 <= r <= 4)
                if in_zone:
                    our_pawn_attacks = pawn_attacks_white(us["pawn"]) if color == "white" else pawn_attacks_black(us["pawn"])
                    add(coeffs, "OUTPOST_SUPPORTED" if our_pawn_attacks & bb(sq) else "OUTPOST_UNSUPPORTED", sign * phase / 256.0)

        bits = us["bishop"]
        while bits:
            sq, bits = pop_lsb(bits)
            mob = min(13, (bishop_attacks(sq, pos.occ) & ~our_occ & ~their_pawn_attacks).bit_count())
            add_pair_coeff(coeffs, f"BISHOP_MOBILITY[{mob}]", sign, phase)
        if us["bishop"].bit_count() >= 2:
            add_pair_coeff(coeffs, "BISHOP_PAIR_BONUS", sign, phase)

        bits = us["rook"]
        while bits:
            sq, bits = pop_lsb(bits)
            mob = min(14, (rook_attacks(sq, pos.occ) & ~our_occ).bit_count())
            add_pair_coeff(coeffs, f"ROOK_MOBILITY[{mob}]", sign, phase)
            file_bb = FILES[file_of(sq)]
            if not (us["pawn"] & file_bb):
                add_pair_coeff(coeffs, "ROOK_OPEN_FILE_BONUS" if not (them["pawn"] & file_bb) else "ROOK_SEMI_OPEN_FILE_BONUS", sign, phase)
            seventh = RANKS[6] if color == "white" else RANKS[1]
            if bb(sq) & seventh:
                add_pair_coeff(coeffs, "ROOK_ON_SEVENTH_BONUS", sign, phase)

        bits = us["queen"]
        while bits:
            sq, bits = pop_lsb(bits)
            mob = min(27, (queen_attacks(sq, pos.occ) & ~our_occ & ~their_pawn_attacks).bit_count())
            add_pair_coeff(coeffs, f"QUEEN_MOBILITY[{mob}]", sign, phase)


def is_backward(color: str, sq: int, rank: int, adj_files: int, our_pawns: int, their_attacks: int) -> bool:
    if not (our_pawns & adj_files):
        return False
    if color == "white":
        if rank >= 7:
            return False
        advance = sq + 8
    else:
        if rank == 0:
            return False
        advance = sq - 8
    if not (bb(advance) & their_attacks):
        return False
    return not (our_pawns & ranks_behind_inclusive(color, rank, adj_files))


def pawn_coeffs(pos: Position, coeffs: dict[int, float], phase: int) -> None:
    for color in ("white", "black"):
        sign = sign_for(color)
        us = pos.white if color == "white" else pos.black
        them = pos.black if color == "white" else pos.white
        their_attacks = pawn_attacks_black(them["pawn"]) if color == "white" else pawn_attacks_white(them["pawn"])
        pawns = us["pawn"]
        bits = pawns
        while bits:
            sq, bits = pop_lsb(bits)
            file = file_of(sq)
            rank = rank_of(sq)
            file_bb = FILES[file]
            adj = (FILES[file - 1] if file > 0 else 0) | (FILES[file + 1] if file < 7 else 0)
            if (pawns & file_bb).bit_count() > 1:
                add_pair_coeff(coeffs, "DOUBLED_PAWN_PENALTY", sign, phase)
            if not (pawns & adj):
                add_pair_coeff(coeffs, "ISOLATED_PAWN_PENALTY", sign, phase)
            promo_dist = 7 - rank if color == "white" else rank
            if not (them["pawn"] & ranks_ahead(color, rank, file_bb | adj)) and not (pawns & ranks_ahead(color, rank, file_bb)):
                adv = min(7, 7 - promo_dist)
                mgw, egw = phase_weight(phase)
                add(coeffs, f"PASSED_PAWN_BONUS_MG[{adv}]", sign * mgw)
                add(coeffs, f"PASSED_PAWN_BONUS_EG[{adv}]", sign * egw)
                if not (pos.occ & ranks_ahead(color, rank, file_bb)):
                    add_pair_coeff(coeffs, "PASSER_PATH_CLEAR_BONUS", sign, phase)
                our_king = pos.white_king if color == "white" else pos.black_king
                their_king = pos.black_king if color == "white" else pos.white_king
                if our_king is not None and their_king is not None:
                    add(coeffs, "PASSER_KING_PROXIMITY_EG", sign * max(0, 4 - chebyshev(our_king, sq)) * egw)
                    add(coeffs, "PASSER_ENEMY_KING_DIST_EG", sign * max(0, chebyshev(their_king, sq) - 3) * egw)
                adj_pawns = pawns & adj
                while adj_pawns:
                    adj_sq, adj_pawns = pop_lsb(adj_pawns)
                    adj_file_bb = FILES[file_of(adj_sq)]
                    if not (them["pawn"] & ranks_ahead(color, rank_of(adj_sq), adj_file_bb | file_bb)):
                        add_pair_coeff(coeffs, "CONNECTED_PASSER_BONUS", sign, phase)
                        break
                if us["rook"] & ranks_behind_inclusive(color, rank, file_bb):
                    add_pair_coeff(coeffs, "ROOK_BEHIND_PASSER_BONUS", sign, phase)
            if is_backward(color, sq, rank, adj, pawns, their_attacks):
                add_pair_coeff(coeffs, "BACKWARD_PAWN_PENALTY", sign, phase)
        protected = (pawn_attacks_white(pawns) if color == "white" else pawn_attacks_black(pawns)) & pawns
        add_pair_coeff(coeffs, "PAWN_CHAIN_BONUS", sign, phase, protected.bit_count())


def king_coeffs(pos: Position, coeffs: dict[int, float], phase: int, include_weights: bool) -> None:
    mgw, egw = phase_weight(phase)
    weight_defaults = {
        "knight": PARAMS[PARAM_INDEX["KING_ATTACK_WEIGHT_KNIGHT"]].value,
        "bishop": PARAMS[PARAM_INDEX["KING_ATTACK_WEIGHT_BISHOP"]].value,
        "rook": PARAMS[PARAM_INDEX["KING_ATTACK_WEIGHT_ROOK"]].value,
        "queen": PARAMS[PARAM_INDEX["KING_ATTACK_WEIGHT_QUEEN"]].value,
    }
    for color in ("white", "black"):
        sign = sign_for(color)
        us = pos.white if color == "white" else pos.black
        them = pos.black if color == "white" else pos.white
        king = pos.white_king if color == "white" else pos.black_king
        if king is None:
            continue
        king_file = file_of(king)
        shield_rank = RANKS[1] | RANKS[2] if color == "white" else RANKS[5] | RANKS[6]
        shield_files = FILES[king_file] | (FILES[king_file - 1] if king_file > 0 else 0) | (FILES[king_file + 1] if king_file < 7 else 0)
        shield_count = (us["pawn"] & shield_rank & shield_files).bit_count()
        add(coeffs, "PAWN_SHIELD_PER_PAWN", sign * shield_count * mgw)
        add(coeffs, "PAWN_SHIELD_BASE_PENALTY", -sign * mgw)

        king_zone = king_attacks(king) | bb(king)
        units = 0
        attackers_by_piece: dict[str, int] = {}
        for piece, attack_fn in (
            ("knight", lambda sq: knight_attacks(sq)),
            ("bishop", lambda sq: bishop_attacks(sq, pos.occ)),
            ("rook", lambda sq: rook_attacks(sq, pos.occ)),
            ("queen", lambda sq: queen_attacks(sq, pos.occ)),
        ):
            count = 0
            bits = them[piece]
            while bits:
                sq, bits = pop_lsb(bits)
                if attack_fn(sq) & king_zone:
                    count += 1
            attackers_by_piece[piece] = count
            units += count * weight_defaults[piece]
        bucket = 0 if units <= 2 else 1 if units <= 5 else 2 if units <= 8 else 3 if units <= 11 else 4 if units <= 15 else 5 if units <= 20 else 6
        add(coeffs, f"KING_SAFETY_TABLE[{bucket}]", -sign * mgw)
        if include_weights:
            # Local linearization around the current bucket. Useful for experiments, off by default.
            for piece, count in attackers_by_piece.items():
                add(coeffs, f"KING_ATTACK_WEIGHT_{piece.upper()}", -sign * count * mgw)

        center_dist = min(abs(3 - king_file), abs(4 - king_file)) + min(abs(3 - rank_of(king)), abs(4 - rank_of(king)))
        add(coeffs, "KING_CENTRALIZATION_EG", sign * max(0, 3 - center_dist) * egw)


def label_from_result(value: str) -> float | None:
    if value == "":
        return None
    score = float(value)
    if score > 0:
        return 1.0
    if score < 0:
        return 0.0
    return 0.5


def sigmoid(k: float, cp: float) -> float:
    return 1.0 / (1.0 + math.exp(max(-60.0, min(60.0, -k * cp))))


def row_error(label: float, cp: float, k: float, target: str) -> float:
    estimate = sigmoid(k, cp) if target == "result" else cp
    diff = estimate - label
    return diff * diff


def contribution(values: list[int], coeffs: dict[int, float]) -> float:
    return sum(values[index] * coeff for index, coeff in coeffs.items())


def read_dataset(args: argparse.Namespace, tune_indices: set[int]) -> Dataset:
    defaults = [param.value for param in PARAMS]
    labels: list[float] = []
    evals: list[float] = []
    rebuild_errors: list[float] = []
    train_rows: list[int] = []
    validation_rows: list[int] = []
    coeffs_by_param: list[dict[int, float]] = [dict() for _ in PARAMS]
    include_weights = any(PARAMS[i].group == "king_weight" for i in tune_indices)
    validation_every = 0
    if args.validation_fraction > 0.0:
        if not 0.0 <= args.validation_fraction < 1.0:
            raise SystemExit("--validation-fraction must be in [0, 1).")
        validation_every = max(2, round(1.0 / args.validation_fraction))

    with open(args.csv_path, newline="", encoding="utf8") as handle:
        reader = csv.DictReader(handle)
        for row in reader:
            if args.target == "result":
                label = label_from_result(row.get("result_score", ""))
                if label is None:
                    continue
            else:
                try:
                    label = float(row["future_white_score_cp"])
                except ValueError:
                    continue
            try:
                phase = int(row["phase"])
                white_score = float(row["white_score_cp"])
            except ValueError:
                continue
            pos = parse_fen(row["fen"])
            coeffs: dict[int, float] = {}
            mobility_coeffs(pos, coeffs, phase)
            pawn_coeffs(pos, coeffs, phase)
            king_coeffs(pos, coeffs, phase, include_weights)

            modeled = contribution(defaults, coeffs)
            component_sum = sum(float(row[name]) for name in (
                "mobility_cp",
                "pawn_structure_cp",
                "king_safety_cp",
            ))
            rebuild_errors.append(modeled - component_sum)
            row_index = len(labels)
            labels.append(label)
            evals.append(white_score)
            if validation_every > 0 and row_index % validation_every == 0:
                validation_rows.append(row_index)
            else:
                train_rows.append(row_index)
            for index, coeff in coeffs.items():
                if index in tune_indices:
                    coeffs_by_param[index][row_index] = coeff
            if args.limit > 0 and len(labels) >= args.limit:
                break
    if not labels:
        raise SystemExit("No usable rows found.")
    if not train_rows:
        raise SystemExit("No training rows found; lower --validation-fraction.")
    return Dataset(labels, evals, coeffs_by_param, rebuild_errors, train_rows, validation_rows)


def parse_steps(value: str) -> list[int]:
    return [int(part.strip()) for part in value.split(",") if part.strip()]


def error_sum_for_rows(dataset: Dataset, errors: list[float], rows: list[int]) -> float:
    return sum(errors[row] for row in rows)


def prior_sum(values: list[int], indices: list[int]) -> float:
    return sum((values[index] - PARAMS[index].value) ** 2 for index in indices)


def clamp_to_default_window(value: int, index: int, max_delta: int) -> int:
    param = PARAMS[index]
    lo = max(param.min_value, param.value - max_delta)
    hi = min(param.max_value, param.value + max_delta)
    return max(lo, min(hi, value))


def set_max(values: list[int], name: str, maximum: int) -> None:
    for suffix in (".mg", ".eg"):
        key = name + suffix
        if key in PARAM_INDEX:
            idx = PARAM_INDEX[key]
            values[idx] = min(values[idx], maximum)


def set_min(values: list[int], name: str, minimum: int) -> None:
    for suffix in (".mg", ".eg"):
        key = name + suffix
        if key in PARAM_INDEX:
            idx = PARAM_INDEX[key]
            values[idx] = max(values[idx], minimum)


def project_mobility_table(values: list[int], name: str) -> None:
    i = 1
    while f"{name}[{i}].mg" in PARAM_INDEX:
        for suffix in (".mg", ".eg"):
            prev = PARAM_INDEX[f"{name}[{i - 1}]{suffix}"]
            current = PARAM_INDEX[f"{name}[{i}]{suffix}"]
            values[current] = max(values[current], values[prev])
        i += 1


def project_array_nonnegative(values: list[int], name: str) -> None:
    i = 0
    while f"{name}[{i}]" in PARAM_INDEX:
        values[PARAM_INDEX[f"{name}[{i}]"]] = max(0, values[PARAM_INDEX[f"{name}[{i}]"]])
        i += 1


def project_king_safety(values: list[int]) -> None:
    for i in range(1, 7):
        prev = PARAM_INDEX[f"KING_SAFETY_TABLE[{i - 1}]"]
        current = PARAM_INDEX[f"KING_SAFETY_TABLE[{i}]"]
        values[current] = max(values[current], values[prev])


def project_values(values: list[int], tune_indices: set[int], args: argparse.Namespace) -> list[int]:
    if args.no_constraints and args.max_delta <= 0:
        return values

    projected = values.copy()
    max_delta = args.max_delta if args.max_delta > 0 else 10_000
    for index in tune_indices:
        projected[index] = clamp_to_default_window(projected[index], index, max_delta)

    if args.no_constraints:
        return projected

    for name in ("DOUBLED_PAWN_PENALTY", "ISOLATED_PAWN_PENALTY", "BACKWARD_PAWN_PENALTY"):
        set_max(projected, name, 0)
    for name in (
        "BISHOP_PAIR_BONUS",
        "ROOK_OPEN_FILE_BONUS",
        "ROOK_SEMI_OPEN_FILE_BONUS",
        "ROOK_ON_SEVENTH_BONUS",
        "PAWN_CHAIN_BONUS",
        "ROOK_BEHIND_PASSER_BONUS",
        "CONNECTED_PASSER_BONUS",
        "PASSER_PATH_CLEAR_BONUS",
    ):
        set_min(projected, name, 0)

    for name in (
        "OUTPOST_SUPPORTED",
        "OUTPOST_UNSUPPORTED",
        "PAWN_SHIELD_PER_PAWN",
        "PAWN_SHIELD_BASE_PENALTY",
        "KING_CENTRALIZATION_EG",
        "PASSER_KING_PROXIMITY_EG",
        "PASSER_ENEMY_KING_DIST_EG",
    ):
        projected[PARAM_INDEX[name]] = max(0, projected[PARAM_INDEX[name]])

    projected[PARAM_INDEX["OUTPOST_SUPPORTED"]] = max(
        projected[PARAM_INDEX["OUTPOST_SUPPORTED"]],
        projected[PARAM_INDEX["OUTPOST_UNSUPPORTED"]],
    )
    for name in ("KNIGHT_MOBILITY", "BISHOP_MOBILITY", "ROOK_MOBILITY", "QUEEN_MOBILITY"):
        project_mobility_table(projected, name)
    for name in ("PASSED_PAWN_BONUS_MG", "PASSED_PAWN_BONUS_EG"):
        project_array_nonnegative(projected, name)
    project_king_safety(projected)

    for index in tune_indices:
        projected[index] = clamp_to_default_window(projected[index], index, max_delta)
    return projected


def candidate_changes(current: list[int], candidate: list[int]) -> dict[int, int]:
    return {
        index: candidate[index] - current[index]
        for index in range(len(current))
        if candidate[index] != current[index]
    }


def affected_rows(dataset: Dataset, deltas: dict[int, int]) -> set[int]:
    rows: set[int] = set()
    for index in deltas:
        rows.update(dataset.coeffs[index])
    return rows


def eval_delta_for_row(dataset: Dataset, row: int, deltas: dict[int, int]) -> float:
    return sum(dataset.coeffs[index].get(row, 0.0) * delta for index, delta in deltas.items())


def try_candidate(
    dataset: Dataset,
    errors: list[float],
    current_values: list[int],
    candidate_values: list[int],
    current_prior: float,
    current_validation_error: float,
    args: argparse.Namespace,
    tune_indices: list[int],
) -> tuple[float, float, float, list[tuple[int, float, float]], list[int]] | None:
    deltas = candidate_changes(current_values, candidate_values)
    if not deltas:
        return None

    changed: list[tuple[int, float, float]] = []
    train_delta_error = 0.0
    validation_delta_error = 0.0
    validation_set = set(dataset.validation_rows)

    for row in affected_rows(dataset, deltas):
        delta_eval = eval_delta_for_row(dataset, row, deltas)
        if delta_eval == 0:
            continue
        new_eval = dataset.evals[row] + delta_eval
        new_error = row_error(dataset.labels[row], new_eval, args.k, args.target)
        delta_error = new_error - errors[row]
        if row in validation_set:
            validation_delta_error += delta_error
        else:
            train_delta_error += delta_error
        changed.append((row, new_eval, new_error))

    new_prior = prior_sum(candidate_values, tune_indices)
    regularized_delta = train_delta_error + len(dataset.train_rows) * args.l2 * (new_prior - current_prior)
    if regularized_delta >= -1e-12:
        return None

    if dataset.validation_rows and not args.no_validation_gate:
        if current_validation_error + validation_delta_error >= current_validation_error - 1e-12:
            return None

    return regularized_delta, train_delta_error, validation_delta_error, changed, candidate_values


def tune(
    dataset: Dataset,
    args: argparse.Namespace,
    tune_indices: list[int],
) -> tuple[list[int], float, float, float, float]:
    values = [param.value for param in PARAMS]
    errors = [row_error(label, cp, args.k, args.target) for label, cp in zip(dataset.labels, dataset.evals)]
    current_train_error = error_sum_for_rows(dataset, errors, dataset.train_rows)
    current_validation_error = error_sum_for_rows(dataset, errors, dataset.validation_rows)
    current_prior = prior_sum(values, tune_indices)
    initial_train_mse = current_train_error / len(dataset.train_rows)
    initial_validation_mse = (
        current_validation_error / len(dataset.validation_rows) if dataset.validation_rows else 0.0
    )
    tune_set = set(tune_indices)

    for step in parse_steps(args.steps):
        for _ in range(args.passes):
            improved = False
            for index in tune_indices:
                current = values[index]
                best: tuple[float, float, float, list[tuple[int, float, float]], list[int]] | None = None
                for direction in (1, -1):
                    raw_candidate = values.copy()
                    raw_candidate[index] = current + direction * step
                    candidate = project_values(raw_candidate, tune_set, args)
                    attempt = try_candidate(
                        dataset,
                        errors,
                        values,
                        candidate,
                        current_prior,
                        current_validation_error,
                        args,
                        tune_indices,
                    )
                    if attempt is not None and (best is None or attempt[0] < best[0]):
                        best = attempt
                if best is None:
                    continue
                regularized_delta, train_delta_error, validation_delta_error, changed, values = best
                current_train_error += train_delta_error
                current_validation_error += validation_delta_error
                current_prior = prior_sum(values, tune_indices)
                for row, new_eval, new_error in changed:
                    dataset.evals[row] = new_eval
                    errors[row] = new_error
                improved = True
            if not improved:
                break
    best_train_mse = current_train_error / len(dataset.train_rows)
    best_validation_mse = (
        current_validation_error / len(dataset.validation_rows) if dataset.validation_rows else 0.0
    )
    return values, initial_train_mse, best_train_mse, initial_validation_mse, best_validation_mse


def selected_indices(groups_arg: str) -> list[int]:
    groups = {part.strip() for part in groups_arg.split(",") if part.strip()}
    if "all" in groups:
        groups = {param.group for param in PARAMS}
    known = {param.group for param in PARAMS}
    unknown = sorted(groups - known)
    if unknown:
        raise SystemExit(f"Unknown group(s): {', '.join(unknown)}. Known groups: {', '.join(sorted(known))}.")
    return [i for i, param in enumerate(PARAMS) if param.group in groups]


def print_changed(values: list[int], indices: list[int]) -> None:
    print("\nChanged parameters:")
    for index in indices:
        param = PARAMS[index]
        if values[index] != param.value:
            print(f"{param.name},{param.value},{values[index]}")


def print_pair_const(name: str, values: list[int]) -> None:
    print(f"const {name}: (i32, i32) = ({values[PARAM_INDEX[name + '.mg']]}, {values[PARAM_INDEX[name + '.eg']]});")


def print_scalar_const(name: str, values: list[int]) -> None:
    print(f"const {name}: i32 = {values[PARAM_INDEX[name]]};")


def print_array(name: str, values: list[int], kind: str = "[(i32, i32)]") -> None:
    if kind == "[(i32, i32)]":
        rows = []
        i = 0
        while f"{name}[{i}].mg" in PARAM_INDEX:
            rows.append((values[PARAM_INDEX[f"{name}[{i}].mg"]], values[PARAM_INDEX[f"{name}[{i}].eg"]]))
            i += 1
        print(f"const {name}: [(i32, i32); {len(rows)}] = [")
        for mg, eg in rows:
            print(f"    ({mg}, {eg}),")
        print("];")
    else:
        rows = []
        i = 0
        while f"{name}[{i}]" in PARAM_INDEX:
            rows.append(values[PARAM_INDEX[f"{name}[{i}]"]])
            i += 1
        print(f"const {name}: [i32; {len(rows)}] = {rows};")


def print_rust(values: list[int]) -> None:
    print("\nRust replacements:")
    for name in ("BISHOP_PAIR_BONUS", "ROOK_OPEN_FILE_BONUS", "ROOK_SEMI_OPEN_FILE_BONUS", "ROOK_ON_SEVENTH_BONUS"):
        print_pair_const(name, values)
    for name in ("OUTPOST_SUPPORTED", "OUTPOST_UNSUPPORTED"):
        print_scalar_const(name, values)
    for name in ("KNIGHT_MOBILITY", "BISHOP_MOBILITY", "ROOK_MOBILITY", "QUEEN_MOBILITY"):
        print_array(name, values)
    for name in ("DOUBLED_PAWN_PENALTY", "ISOLATED_PAWN_PENALTY", "BACKWARD_PAWN_PENALTY", "PAWN_CHAIN_BONUS"):
        print_pair_const(name, values)
    for name in ("PASSED_PAWN_BONUS_MG", "PASSED_PAWN_BONUS_EG"):
        print_array(name, values, "[i32]")
    for name in ("PAWN_SHIELD_PER_PAWN", "PAWN_SHIELD_BASE_PENALTY", "KING_ATTACK_WEIGHT_KNIGHT", "KING_ATTACK_WEIGHT_BISHOP", "KING_ATTACK_WEIGHT_ROOK", "KING_ATTACK_WEIGHT_QUEEN"):
        print_scalar_const(name, values)
    print("const KING_SAFETY_TABLE: [(i32, i32); 7] = [")
    for limit, i in zip([2, 5, 8, 11, 15, 20, "i32::MAX"], range(7)):
        print(f"    ({limit}, {values[PARAM_INDEX[f'KING_SAFETY_TABLE[{i}]']]}),")
    print("];")
    for name in (
        "ROOK_BEHIND_PASSER_BONUS",
        "CONNECTED_PASSER_BONUS",
        "PASSER_PATH_CLEAR_BONUS",
    ):
        print_pair_const(name, values)
    for name in ("KING_CENTRALIZATION_EG", "PASSER_KING_PROXIMITY_EG", "PASSER_ENEMY_KING_DIST_EG"):
        print_scalar_const(name, values)


def main() -> None:
    args = parse_args()
    indices = selected_indices(args.groups)
    dataset = read_dataset(args, set(indices))
    values, initial_train_mse, best_train_mse, initial_validation_mse, best_validation_mse = tune(
        dataset, args, indices
    )
    abs_errors = [abs(x) for x in dataset.rebuild_errors]
    print(f"rows={len(dataset.labels)}")
    print(f"train_rows={len(dataset.train_rows)}")
    print(f"validation_rows={len(dataset.validation_rows)}")
    print(f"target={args.target}")
    print(f"k={args.k}")
    print(f"params={len(indices)}")
    print(f"l2={args.l2}")
    print(f"max_delta={args.max_delta}")
    print(f"constraints={not args.no_constraints}")
    print(f"validation_gate={not args.no_validation_gate and bool(dataset.validation_rows)}")
    print(f"rebuild_mean_abs_cp={sum(abs_errors) / len(abs_errors):.6f}")
    print(f"rebuild_max_abs_cp={max(abs_errors):.6f}")
    print(f"initial_train_mse={initial_train_mse:.8f}")
    print(f"best_train_mse={best_train_mse:.8f}")
    print(f"delta_train_mse={initial_train_mse - best_train_mse:.8f}")
    if dataset.validation_rows:
        print(f"initial_validation_mse={initial_validation_mse:.8f}")
        print(f"best_validation_mse={best_validation_mse:.8f}")
        print(f"delta_validation_mse={initial_validation_mse - best_validation_mse:.8f}")
    print_changed(values, indices)
    print_rust(values)


if __name__ == "__main__":
    main()
