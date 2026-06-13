#!/usr/bin/env python3
"""
Karpov Match Manager — local web GUI for engine-vs-engine matches.

A snapshot library of engine builds + a cutechess-cli match runner with a
live-updating dashboard and game replay.

Usage:
    python3 tools/match_manager/server.py [--port 8787]

Then open http://localhost:8787 in your browser.
"""

import argparse
import json
import math
import os
import re
import shutil
import stat
import subprocess
import threading
from datetime import datetime
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse

import chess
import chess.pgn

# ── Paths ────────────────────────────────────────────────────────────────────

MM_DIR = os.path.dirname(os.path.abspath(__file__))
TOOLS_DIR = os.path.dirname(MM_DIR)
ROOT = os.path.dirname(TOOLS_DIR)
STATIC_DIR = os.path.join(MM_DIR, "static")
ENGINES_DIR = os.path.join(MM_DIR, "engines")
MATCHES_DIR = os.path.join(MM_DIR, "matches")
CUTECHESS = os.path.join(TOOLS_DIR, "cutechess-cli")
OPENINGS = os.path.join(TOOLS_DIR, "openings.epd")
KARPOV_BIN = os.path.join(ROOT, "target", "release", "karpov")
STOCKFISH = shutil.which("stockfish") or "/usr/games/stockfish"

DEFAULT_CONCURRENCY = max(1, min(20, (os.cpu_count() or 4) - 4))

os.makedirs(ENGINES_DIR, exist_ok=True)
os.makedirs(MATCHES_DIR, exist_ok=True)


# ── Helpers ──────────────────────────────────────────────────────────────────

def timestamp():
    return datetime.now().strftime("%Y%m%d_%H%M%S")


def safe_name(name):
    """Sanitize a user-supplied label for use as a directory name."""
    name = re.sub(r"[^A-Za-z0-9_.+-]", "_", name.strip())
    return name[:64]


def read_json(path, default=None):
    try:
        with open(path) as f:
            return json.load(f)
    except (OSError, json.JSONDecodeError):
        return default


def write_json(path, data):
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        json.dump(data, f, indent=2)
    os.replace(tmp, path)


def elo_from_score(score):
    if score <= 0:
        return float("-inf")
    if score >= 1:
        return float("inf")
    return -400.0 * math.log10(1.0 / score - 1.0)


def elo_stats(wins, draws, losses):
    """Elo difference + 95% error margin from a W/D/L record."""
    n = wins + draws + losses
    if n == 0:
        return None, None, None
    score = (wins + 0.5 * draws) / n
    elo = elo_from_score(score)
    # Per-game score variance → 95% CI on the score, mapped through the Elo curve
    var = (wins * (1 - score) ** 2 + draws * (0.5 - score) ** 2 + losses * score**2) / n
    stdev = math.sqrt(var / n) if n > 0 else 0.0
    lo = max(score - 1.959964 * stdev, 1e-6)
    hi = min(score + 1.959964 * stdev, 1 - 1e-6)
    err = (elo_from_score(hi) - elo_from_score(lo)) / 2
    los = 0.5 * (1 + math.erf((wins - losses) / math.sqrt(2.0 * max(wins + losses, 1))))
    if math.isinf(elo):
        return (9999.0 if elo > 0 else -9999.0), None, round(los * 100, 1)
    return round(elo, 1), round(err, 1) if err else None, round(los * 100, 1)


# ── Engine library ───────────────────────────────────────────────────────────

build_lock = threading.Lock()


def list_engines():
    engines = []
    for name in sorted(os.listdir(ENGINES_DIR)):
        meta = read_json(os.path.join(ENGINES_DIR, name, "meta.json"))
        binary = os.path.join(ENGINES_DIR, name, "karpov")
        if meta and os.path.isfile(binary):
            engines.append(meta)
    return engines


def snapshot_engine(name, note=""):
    """Build the current source and store the binary under a label."""
    name = safe_name(name)
    if not name:
        raise ValueError("Snapshot name is required")
    dest_dir = os.path.join(ENGINES_DIR, name)
    if os.path.exists(dest_dir):
        raise ValueError(f"Snapshot '{name}' already exists")

    with build_lock:
        result = subprocess.run(
            ["cargo", "build", "--release"],
            capture_output=True, text=True, cwd=ROOT, timeout=300,
        )
    if result.returncode != 0:
        raise RuntimeError("cargo build failed:\n" + result.stderr[-2000:])

    os.makedirs(dest_dir)
    dest_bin = os.path.join(dest_dir, "karpov")
    shutil.copy2(KARPOV_BIN, dest_bin)
    os.chmod(dest_bin, os.stat(dest_bin).st_mode | stat.S_IEXEC)
    meta = {
        "name": name,
        "note": note,
        "created": datetime.now().isoformat(timespec="seconds"),
        "source": "cargo build --release",
    }
    write_json(os.path.join(dest_dir, "meta.json"), meta)
    return meta


def import_engine(name, path, note=""):
    """Import an existing binary into the library."""
    name = safe_name(name)
    if not name:
        raise ValueError("Name is required")
    path = os.path.abspath(os.path.expanduser(path))
    if not os.path.isfile(path):
        raise ValueError(f"No such file: {path}")
    dest_dir = os.path.join(ENGINES_DIR, name)
    if os.path.exists(dest_dir):
        raise ValueError(f"Snapshot '{name}' already exists")
    os.makedirs(dest_dir)
    dest_bin = os.path.join(dest_dir, "karpov")
    shutil.copy2(path, dest_bin)
    os.chmod(dest_bin, os.stat(dest_bin).st_mode | stat.S_IEXEC)
    meta = {
        "name": name,
        "note": note,
        "created": datetime.now().isoformat(timespec="seconds"),
        "source": path,
    }
    write_json(os.path.join(dest_dir, "meta.json"), meta)
    return meta


def delete_engine(name):
    name = safe_name(name)
    for match in matches.values():
        if match.status == "running" and name in (
            match.config.get("white", {}).get("name"),
            match.config.get("black", {}).get("name"),
        ):
            raise ValueError(f"'{name}' is used by a running match")
    dest_dir = os.path.join(ENGINES_DIR, name)
    if not os.path.isdir(dest_dir):
        raise ValueError(f"No snapshot named '{name}'")
    shutil.rmtree(dest_dir)


# ── Match runner ─────────────────────────────────────────────────────────────

SCORE_RE = re.compile(r"Score of (.+?) vs (.+?): (\d+) - (\d+) - (\d+)\s+\[([0-9.]+)\]\s+(\d+)")
FINISHED_RE = re.compile(r"Finished game (\d+)")
SPRT_RE = re.compile(r"SPRT: llr ([-0-9.]+)")


def parse_extra_options(text):
    """Parse 'Key=Val, Key=Val' into a dict."""
    opts = {}
    for part in (text or "").split(","):
        part = part.strip()
        if "=" in part:
            k, v = part.split("=", 1)
            opts[k.strip()] = v.strip()
    return opts


def resolve_engine_spec(spec, settings, used_names):
    """Turn an engine spec into (cmd, display_name, uci_options)."""
    if spec["type"] == "stockfish":
        elo = int(spec.get("elo", 2000))
        name = f"SF_{elo}"
        opts = {
            "UCI_LimitStrength": "true",
            "UCI_Elo": str(elo),
            "Threads": "1",
            "Hash": "16",
        }
        cmd = STOCKFISH
    else:
        sname = safe_name(spec["name"])
        cmd = os.path.join(ENGINES_DIR, sname, "karpov")
        if not os.path.isfile(cmd):
            raise ValueError(f"No snapshot named '{sname}'")
        name = sname
        opts = {"Hash": str(settings.get("hash", 128))}
    opts.update(parse_extra_options(spec.get("extra_options", "")))
    # cutechess requires distinct engine names
    while name in used_names:
        name += "_2"
    used_names.add(name)
    return cmd, name, opts


class Match:
    def __init__(self, match_id, config, restore_status=None):
        self.id = match_id
        self.config = config
        self.dir = os.path.join(MATCHES_DIR, match_id)
        self.pgn_path = os.path.join(self.dir, "games.pgn")
        self.log_path = os.path.join(self.dir, "cutechess.log")
        self.lock = threading.Lock()
        self.process = None
        self.thread = None
        self.status = "pending"
        self.error = None
        self.results = {
            "name1": None, "name2": None,
            "wins": 0, "losses": 0, "draws": 0, "games_done": 0,
            "elo": None, "elo_error": None, "los": None,
            "sprt_llr": None, "sprt_result": None,
        }
        self.started = None
        self.finished = None
        if restore_status:
            self.status = restore_status.get("status", "interrupted")
            self.error = restore_status.get("error")
            self.results.update(restore_status.get("results", {}))
            self.started = restore_status.get("started")
            self.finished = restore_status.get("finished")
            if self.status == "running":  # server restarted mid-match
                self.status = "interrupted"

    # ── persistence ──

    def persist(self):
        write_json(os.path.join(self.dir, "status.json"), {
            "status": self.status,
            "error": self.error,
            "results": self.results,
            "started": self.started,
            "finished": self.finished,
        })

    # ── lifecycle ──

    def build_command(self):
        cfg = self.config
        s = cfg["settings"]
        used_names = set()
        cmd1, name1, opts1 = resolve_engine_spec(cfg["white"], s, used_names)
        cmd2, name2, opts2 = resolve_engine_spec(cfg["black"], s, used_names)
        self.results["name1"], self.results["name2"] = name1, name2

        cmd = [CUTECHESS]
        for ecmd, ename, eopts in ((cmd1, name1, opts1), (cmd2, name2, opts2)):
            parts = [f"cmd={ecmd}", "proto=uci", f"name={ename}"]
            parts += [f"option.{k}={v}" for k, v in eopts.items()]
            cmd.extend(["-engine"] + parts)

        cmd.extend(["-each", "proto=uci", f"tc={s['tc']}"])
        games = max(2, int(s.get("games", 100)))
        cmd.extend(["-games", "2", "-rounds", str((games + 1) // 2), "-repeat"])
        cmd.extend(["-concurrency", str(int(s.get("concurrency", DEFAULT_CONCURRENCY)))])
        if s.get("openings", True):
            cmd.extend(["-openings", f"file={OPENINGS}", "format=epd",
                        "order=random", "policy=round"])
        cmd.extend(["-recover", "-maxmoves", "200"])
        if s.get("draw_adjudication", True):
            cmd.extend(["-draw", "movenumber=40", "movecount=8", "score=10"])
        if s.get("resign_adjudication", True):
            cmd.extend(["-resign", "movecount=5", "score=700", "twosided=true"])
        if s.get("sprt"):
            cmd.extend(["-sprt", f"elo0={s.get('sprt_elo0', 0)}",
                        f"elo1={s.get('sprt_elo1', 5)}", "alpha=0.05", "beta=0.05"])
        cmd.extend(["-pgnout", self.pgn_path])
        cmd.extend(["-ratinginterval", "10"])
        return cmd

    def start(self):
        os.makedirs(self.dir, exist_ok=True)
        write_json(os.path.join(self.dir, "config.json"), self.config)
        try:
            cmd = self.build_command()
        except ValueError as e:
            self.status = "error"
            self.error = str(e)
            self.persist()
            return
        self.status = "running"
        self.started = datetime.now().isoformat(timespec="seconds")
        self.persist()
        self.thread = threading.Thread(target=self._run, args=(cmd,), daemon=True)
        self.thread.start()

    def _run(self, cmd):
        try:
            with open(self.log_path, "w") as log:
                log.write(" ".join(cmd) + "\n\n")
                log.flush()
                self.process = subprocess.Popen(
                    cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                )
                for line in self.process.stdout:
                    log.write(line)
                    log.flush()
                    self._ingest(line.rstrip())
                self.process.wait()
            with self.lock:
                if self.status == "running":
                    self.status = "finished" if self.process.returncode == 0 else "error"
                    if self.status == "error":
                        self.error = f"cutechess exited with code {self.process.returncode}"
        except Exception as e:  # noqa: BLE001 — surface anything to the UI
            with self.lock:
                self.status = "error"
                self.error = str(e)
        self.finished = datetime.now().isoformat(timespec="seconds")
        self.persist()

    def _ingest(self, line):
        with self.lock:
            r = self.results
            m = SCORE_RE.search(line)
            if m:
                r["wins"], r["losses"], r["draws"] = int(m.group(3)), int(m.group(4)), int(m.group(5))
                r["games_done"] = int(m.group(7))
                r["elo"], r["elo_error"], r["los"] = elo_stats(r["wins"], r["draws"], r["losses"])
            m = FINISHED_RE.search(line)
            if m:
                r["games_done"] = max(r["games_done"], int(m.group(1)))
            m = SPRT_RE.search(line)
            if m:
                r["sprt_llr"] = float(m.group(1))
            if "SPRT: llr" in line:
                if "H1 was accepted" in line:
                    r["sprt_result"] = "PASSED"
                elif "H0 was accepted" in line:
                    r["sprt_result"] = "FAILED"
        # persist occasionally so restarts keep recent results
        if self.results["games_done"] % 10 == 0:
            self.persist()

    def stop(self):
        with self.lock:
            if self.status == "running" and self.process:
                self.process.terminate()
                self.status = "stopped"
        self.persist()

    # ── views ──

    def summary(self):
        with self.lock:
            return {
                "id": self.id,
                "white": self.config["white"],
                "black": self.config["black"],
                "settings": self.config["settings"],
                "status": self.status,
                "error": self.error,
                "started": self.started,
                "finished": self.finished,
                "results": dict(self.results),
            }

    def games_list(self):
        """Headers-only parse of the PGN for the games table."""
        games = []
        if not os.path.isfile(self.pgn_path):
            return games
        with open(self.pgn_path, encoding="utf-8", errors="replace") as f:
            idx = 0
            while True:
                headers = chess.pgn.read_headers(f)
                if headers is None:
                    break
                games.append({
                    "index": idx,
                    "white": headers.get("White", "?"),
                    "black": headers.get("Black", "?"),
                    "result": headers.get("Result", "*"),
                    "plies": headers.get("PlyCount", "?"),
                    "termination": headers.get("Termination", "normal"),
                    "round": headers.get("Round", "?"),
                })
                idx += 1
        return games

    def game_detail(self, index):
        """Full parse of one game → SANs + FEN after every move, for replay."""
        if not os.path.isfile(self.pgn_path):
            return None
        with open(self.pgn_path, encoding="utf-8", errors="replace") as f:
            game = None
            for _ in range(index + 1):
                game = chess.pgn.read_game(f)
                if game is None:
                    return None
        board = game.board()
        fens = [board.fen()]
        sans = []
        for move in game.mainline_moves():
            sans.append(board.san(move))
            board.push(move)
            fens.append(board.fen())
        return {
            "headers": dict(game.headers),
            "sans": sans,
            "fens": fens,
        }


matches = {}


def load_existing_matches():
    for match_id in sorted(os.listdir(MATCHES_DIR)):
        mdir = os.path.join(MATCHES_DIR, match_id)
        config = read_json(os.path.join(mdir, "config.json"))
        if not config:
            continue
        status = read_json(os.path.join(mdir, "status.json"), {})
        matches[match_id] = Match(match_id, config, restore_status=status)


def start_match(config):
    match_id = "m_" + timestamp()
    while match_id in matches:
        match_id += "x"
    match = Match(match_id, config)
    matches[match_id] = match
    match.start()
    return match


def delete_match(match_id):
    match = matches.get(match_id)
    if not match:
        raise ValueError("No such match")
    if match.status == "running":
        raise ValueError("Stop the match before deleting it")
    shutil.rmtree(match.dir, ignore_errors=True)
    del matches[match_id]


# ── HTTP server ──────────────────────────────────────────────────────────────

MIME = {".html": "text/html", ".js": "application/javascript", ".css": "text/css",
        ".svg": "image/svg+xml", ".png": "image/png"}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):  # silence per-request logging
        pass

    # ── plumbing ──

    def send_json(self, data, code=200):
        body = json.dumps(data).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def send_error_json(self, message, code=400):
        self.send_json({"error": message}, code)

    def read_body(self):
        length = int(self.headers.get("Content-Length", 0))
        if length == 0:
            return {}
        return json.loads(self.rfile.read(length).decode())

    def serve_static(self, path):
        if path == "/":
            path = "/index.html"
        fpath = os.path.normpath(os.path.join(STATIC_DIR, path.lstrip("/")))
        if not fpath.startswith(STATIC_DIR) or not os.path.isfile(fpath):
            self.send_error_json("Not found", 404)
            return
        with open(fpath, "rb") as f:
            body = f.read()
        self.send_response(200)
        self.send_header("Content-Type", MIME.get(os.path.splitext(fpath)[1], "application/octet-stream"))
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    # ── routes ──

    def do_GET(self):
        path = urlparse(self.path).path
        try:
            if path == "/api/state":
                self.send_json({
                    "engines": list_engines(),
                    "stockfish_available": os.path.isfile(STOCKFISH),
                    "default_concurrency": DEFAULT_CONCURRENCY,
                    "matches": sorted(
                        (m.summary() for m in matches.values()),
                        key=lambda m: m["id"], reverse=True,
                    ),
                })
            elif m := re.fullmatch(r"/api/matches/([\w-]+)/games/(\d+)", path):
                match = matches.get(m.group(1))
                if not match:
                    return self.send_error_json("No such match", 404)
                detail = match.game_detail(int(m.group(2)))
                if detail is None:
                    return self.send_error_json("No such game", 404)
                self.send_json(detail)
            elif m := re.fullmatch(r"/api/matches/([\w-]+)", path):
                match = matches.get(m.group(1))
                if not match:
                    return self.send_error_json("No such match", 404)
                data = match.summary()
                data["games"] = match.games_list()
                self.send_json(data)
            else:
                self.serve_static(path)
        except Exception as e:  # noqa: BLE001
            self.send_error_json(str(e), 500)

    def do_POST(self):
        path = urlparse(self.path).path
        try:
            body = self.read_body()
            if path == "/api/engines/snapshot":
                meta = snapshot_engine(body.get("name", ""), body.get("note", ""))
                self.send_json(meta)
            elif path == "/api/engines/import":
                meta = import_engine(body.get("name", ""), body.get("path", ""), body.get("note", ""))
                self.send_json(meta)
            elif path == "/api/matches":
                match = start_match({
                    "white": body["white"],
                    "black": body["black"],
                    "settings": body.get("settings", {}),
                })
                self.send_json(match.summary())
            elif m := re.fullmatch(r"/api/matches/([\w-]+)/stop", path):
                match = matches.get(m.group(1))
                if not match:
                    return self.send_error_json("No such match", 404)
                match.stop()
                self.send_json(match.summary())
            else:
                self.send_error_json("Not found", 404)
        except (ValueError, KeyError) as e:
            self.send_error_json(str(e), 400)
        except Exception as e:  # noqa: BLE001
            self.send_error_json(str(e), 500)

    def do_DELETE(self):
        path = urlparse(self.path).path
        try:
            if m := re.fullmatch(r"/api/engines/([\w.+-]+)", path):
                delete_engine(m.group(1))
                self.send_json({"ok": True})
            elif m := re.fullmatch(r"/api/matches/([\w-]+)", path):
                delete_match(m.group(1))
                self.send_json({"ok": True})
            else:
                self.send_error_json("Not found", 404)
        except ValueError as e:
            self.send_error_json(str(e), 400)
        except Exception as e:  # noqa: BLE001
            self.send_error_json(str(e), 500)


def main():
    parser = argparse.ArgumentParser(description="Karpov Match Manager")
    parser.add_argument("--port", type=int, default=8787)
    parser.add_argument("--host", default="127.0.0.1")
    args = parser.parse_args()

    load_existing_matches()
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(f"Karpov Match Manager → http://{args.host}:{args.port}")
    print(f"  engines: {ENGINES_DIR}")
    print(f"  matches: {MATCHES_DIR}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down…")
        for match in matches.values():
            if match.status == "running":
                match.stop()


if __name__ == "__main__":
    main()
