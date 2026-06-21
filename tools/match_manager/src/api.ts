import fs from "node:fs";
import type {IncomingMessage, ServerResponse} from "node:http";
import path from "node:path";
import {
  defaultSettings,
  importEngine,
  listEngines,
  MatchStore,
  paths,
  snapshotEngine,
} from "./core.js";
import type {EngineSpec, MatchConfig, MatchSettings} from "./types.js";

type ApiOptions = {
  store: MatchStore;
  version: string;
};

type ApiHandle = {
  handle: (req: IncomingMessage, res: ServerResponse) => Promise<boolean>;
  broadcast: () => void;
};

const MAX_BODY_BYTES = 1024 * 1024;

export function createApi({store, version}: ApiOptions): ApiHandle {
  const sseClients = new Set<ServerResponse>();

  const sendEvent = (res: ServerResponse) => {
    res.write(`data: ${JSON.stringify({type: "store:update", matches: store.summaries()})}\n\n`);
  };

  const broadcast = () => {
    for (const client of sseClients) sendEvent(client);
  };

  return {
    broadcast,
    handle: async (req, res) => {
      const parsed = new URL(req.url ?? "/", "http://127.0.0.1");
      if (!parsed.pathname.startsWith("/api/")) return false;
      setCors(req, res);
      if (req.method === "OPTIONS") {
        res.writeHead(204).end();
        return true;
      }

      try {
        if (req.method === "GET" && parsed.pathname === "/api/events") {
          res.writeHead(200, {
            "Content-Type": "text/event-stream; charset=utf-8",
            "Cache-Control": "no-cache, no-transform",
            Connection: "keep-alive",
            "X-Accel-Buffering": "no",
          });
          sseClients.add(res);
          sendEvent(res);
          req.on("close", () => sseClients.delete(res));
          return true;
        }

        if (req.method === "GET" && parsed.pathname === "/api/health") {
          json(res, 200, {ok: true, version});
          return true;
        }

        if (req.method === "GET" && parsed.pathname === "/api/engines") {
          json(res, 200, listEngines());
          return true;
        }

        if (req.method === "POST" && parsed.pathname === "/api/engines/snapshot") {
          const body = await readJsonBody<{name?: string; note?: string}>(req);
          json(res, 201, await snapshotEngine(String(body.name ?? ""), String(body.note ?? "")));
          broadcast();
          return true;
        }

        if (req.method === "POST" && parsed.pathname === "/api/engines/import") {
          const body = await readJsonBody<{name?: string; binary?: string; note?: string}>(req);
          json(res, 201, importEngine(String(body.name ?? ""), String(body.binary ?? ""), String(body.note ?? "")));
          broadcast();
          return true;
        }

        const engineDelete = /^\/api\/engines\/([^/]+)$/.exec(parsed.pathname);
        if (req.method === "DELETE" && engineDelete) {
          store.deleteEngine(decodeURIComponent(engineDelete[1]));
          json(res, 200, {ok: true});
          broadcast();
          return true;
        }

        if (req.method === "GET" && parsed.pathname === "/api/matches") {
          json(res, 200, store.summaries());
          return true;
        }

        if (req.method === "POST" && parsed.pathname === "/api/matches") {
          const config = normalizeMatchConfig(await readJsonBody<Partial<MatchConfig>>(req));
          const match = store.start(config);
          json(res, 201, {id: match.id});
          broadcast();
          return true;
        }

        const fullPgn = /^\/api\/matches\/([^/]+)\/games\.pgn$/.exec(parsed.pathname);
        if (req.method === "GET" && fullPgn) {
          const match = store.get(decodeURIComponent(fullPgn[1]));
          if (!match) throw httpError(404, "No such match");
          sendFile(res, match.pgnPath, "application/x-chess-pgn; charset=utf-8", `${match.id}.pgn`);
          return true;
        }

        const onePgn = /^\/api\/matches\/([^/]+)\/games\/(\d+)\.pgn$/.exec(parsed.pathname);
        if (req.method === "GET" && onePgn) {
          const match = store.get(decodeURIComponent(onePgn[1]));
          if (!match) throw httpError(404, "No such match");
          const detail = match.gameDetail(Number(onePgn[2]));
          if (!detail) throw httpError(404, "No such game");
          text(res, 200, detail.pgn, "application/x-chess-pgn; charset=utf-8", `${match.id}-game-${onePgn[2]}.pgn`);
          return true;
        }

        const gameDetail = /^\/api\/matches\/([^/]+)\/games\/(\d+)$/.exec(parsed.pathname);
        if (req.method === "GET" && gameDetail) {
          const match = store.get(decodeURIComponent(gameDetail[1]));
          if (!match) throw httpError(404, "No such match");
          const detail = match.gameDetail(Number(gameDetail[2]));
          if (!detail) throw httpError(404, "No such game");
          json(res, 200, detail);
          return true;
        }

        const games = /^\/api\/matches\/([^/]+)\/games$/.exec(parsed.pathname);
        if (req.method === "GET" && games) {
          const match = store.get(decodeURIComponent(games[1]));
          if (!match) throw httpError(404, "No such match");
          json(res, 200, match.gamesList());
          return true;
        }

        const stop = /^\/api\/matches\/([^/]+)\/stop$/.exec(parsed.pathname);
        if (req.method === "POST" && stop) {
          const match = store.get(decodeURIComponent(stop[1]));
          if (!match) throw httpError(404, "No such match");
          match.stop();
          json(res, 200, {ok: true});
          broadcast();
          return true;
        }

        const matchRoute = /^\/api\/matches\/([^/]+)$/.exec(parsed.pathname);
        if (matchRoute) {
          const id = decodeURIComponent(matchRoute[1]);
          if (req.method === "GET") {
            const match = store.get(id);
            if (!match) throw httpError(404, "No such match");
            json(res, 200, {summary: match.summary(), games: match.gamesList()});
            return true;
          }
          if (req.method === "DELETE") {
            store.deleteMatch(id);
            json(res, 200, {ok: true});
            broadcast();
            return true;
          }
        }

        json(res, 404, {error: "Not found"});
        return true;
      } catch (error) {
        const status = error instanceof HttpError ? error.status : 500;
        json(res, status, {error: error instanceof Error ? error.message : String(error)});
        return true;
      }
    },
  };
}

export const WEB_DIST = path.join(paths.matchManager, "web", "dist");

function normalizeMatchConfig(input: Partial<MatchConfig>): MatchConfig {
  if (!input.white || !input.black) throw httpError(400, "Both white and black engines are required");
  return {
    white: normalizeEngine(input.white),
    black: normalizeEngine(input.black),
    settings: normalizeSettings(input.settings ?? {}),
  };
}

function normalizeEngine(input: EngineSpec): EngineSpec {
  if (input.type === "stockfish") return {type: "stockfish", elo: Number(input.elo || 2000), extra_options: input.extra_options};
  if (input.type === "snapshot") return {type: "snapshot", name: String(input.name ?? ""), extra_options: input.extra_options};
  throw httpError(400, "Invalid engine type");
}

function normalizeSettings(input: Partial<MatchSettings>): MatchSettings {
  const settings = {...defaultSettings(), ...input};
  settings.games = Math.max(2, Number(settings.games || 100));
  settings.concurrency = Math.max(1, Number(settings.concurrency || 1));
  settings.hash = Math.max(1, Number(settings.hash || 1));
  settings.tc = String(settings.tc || "").trim();
  if (!settings.tc) throw httpError(400, "Time control is required");
  settings.sprt_elo0 = Number(settings.sprt_elo0 || 0);
  settings.sprt_elo1 = Number(settings.sprt_elo1 || 5);
  return settings;
}

function setCors(req: IncomingMessage, res: ServerResponse): void {
  const origin = req.headers.origin;
  const allowed = origin && /^http:\/\/(127\.0\.0\.1|localhost):\d+$/.test(origin)
    ? origin
    : "http://127.0.0.1:5173";
  res.setHeader("Access-Control-Allow-Origin", allowed);
  res.setHeader("Vary", "Origin");
  res.setHeader("Access-Control-Allow-Methods", "GET,POST,DELETE,OPTIONS");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type");
}

function json(res: ServerResponse, status: number, value: unknown): void {
  res.writeHead(status, {"Content-Type": "application/json; charset=utf-8"});
  res.end(`${JSON.stringify(value)}\n`);
}

function text(res: ServerResponse, status: number, body: string, contentType = "text/plain; charset=utf-8", filename?: string): void {
  const headers: Record<string, string> = {"Content-Type": contentType};
  if (filename) headers["Content-Disposition"] = `attachment; filename="${filename.replace(/[^A-Za-z0-9_.-]/g, "_")}"`;
  res.writeHead(status, headers);
  res.end(body);
}

function sendFile(res: ServerResponse, file: string, contentType: string, filename?: string): void {
  if (!fs.existsSync(file)) throw httpError(404, "File not found");
  const headers: Record<string, string> = {"Content-Type": contentType};
  if (filename) headers["Content-Disposition"] = `attachment; filename="${filename.replace(/[^A-Za-z0-9_.-]/g, "_")}"`;
  res.writeHead(200, headers);
  fs.createReadStream(file).pipe(res);
}

function readJsonBody<T>(req: IncomingMessage): Promise<T> {
  return new Promise((resolve, reject) => {
    let raw = "";
    req.setEncoding("utf8");
    req.on("data", (chunk: string) => {
      raw += chunk;
      if (raw.length > MAX_BODY_BYTES) reject(httpError(413, "Request body too large"));
    });
    req.on("error", reject);
    req.on("end", () => {
      try {
        resolve((raw ? JSON.parse(raw) : {}) as T);
      } catch {
        reject(httpError(400, "Invalid JSON body"));
      }
    });
  });
}

class HttpError extends Error {
  constructor(readonly status: number, message: string) {
    super(message);
  }
}

function httpError(status: number, message: string): HttpError {
  return new HttpError(status, message);
}
