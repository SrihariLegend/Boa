#!/usr/bin/env node
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import {fileURLToPath} from "node:url";
import {createApi, WEB_DIST} from "./api.js";
import {MatchStore} from "./core.js";

type ServerOptions = {
  host?: string;
  port?: number;
};

const VERSION = "1.0.0";

export async function startWebServer(options: ServerOptions = {}): Promise<http.Server> {
  const host = options.host ?? "127.0.0.1";
  const port = options.port ?? 3777;
  const store = new MatchStore();
  store.load();

  const api = createApi({store, version: VERSION});
  store.on("change", api.broadcast);
  const refresh = setInterval(api.broadcast, 1500);

  const server = http.createServer(async (req, res) => {
    if (await api.handle(req, res)) return;
    serveStatic(req, res);
  });

  server.on("close", () => {
    clearInterval(refresh);
    store.stopAll();
  });

  await new Promise<void>((resolve) => server.listen(port, host, resolve));
  console.log(`Boa Match Manager web server listening on http://${host}:${port}`);
  return server;
}

function serveStatic(req: http.IncomingMessage, res: http.ServerResponse): void {
  if (!fs.existsSync(WEB_DIST)) {
    res.writeHead(200, {"Content-Type": "text/html; charset=utf-8"});
    res.end(`<!doctype html><title>Boa Match Manager</title><body style="font-family:sans-serif;background:#111827;color:#e5e7eb;padding:2rem"><h1>Boa Match Manager API</h1><p>Frontend bundle not found at <code>${WEB_DIST}</code>.</p><p>Try <code>GET /api/health</code> or build the web app when it exists.</p></body>`);
    return;
  }

  const url = new URL(req.url ?? "/", "http://127.0.0.1");
  const requested = url.pathname === "/" ? "index.html" : decodeURIComponent(url.pathname.slice(1));
  const file = path.resolve(WEB_DIST, requested);
  const root = path.resolve(WEB_DIST);
  const target = file.startsWith(`${root}${path.sep}`) || file === root ? file : path.join(root, "index.html");
  const finalFile = fs.existsSync(target) && fs.statSync(target).isFile() ? target : path.join(root, "index.html");
  if (!fs.existsSync(finalFile)) {
    res.writeHead(404).end("Not found");
    return;
  }
  res.writeHead(200, {"Content-Type": contentType(finalFile)});
  fs.createReadStream(finalFile).pipe(res);
}

function contentType(file: string): string {
  switch (path.extname(file)) {
    case ".html": return "text/html; charset=utf-8";
    case ".js": return "text/javascript; charset=utf-8";
    case ".css": return "text/css; charset=utf-8";
    case ".svg": return "image/svg+xml";
    case ".png": return "image/png";
    case ".ico": return "image/x-icon";
    default: return "application/octet-stream";
  }
}

const thisFile = fileURLToPath(import.meta.url);
if (process.argv[1] && path.resolve(process.argv[1]) === thisFile) {
  await startWebServer({
    port: Number(readArg("--port") ?? 3777),
    host: readArg("--host") ?? "127.0.0.1",
  });
}

function readArg(name: string): string | undefined {
  const exact = process.argv.indexOf(name);
  if (exact >= 0) return process.argv[exact + 1];
  const prefix = `${name}=`;
  return process.argv.find((arg) => arg.startsWith(prefix))?.slice(prefix.length);
}
