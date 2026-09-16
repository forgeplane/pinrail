#!/usr/bin/env node
/*
 * A shell for one plugin, in a browser, without the app: serves the plugin
 * directory under the app's CSP, the SDK beside it, and a page that plays
 * the shell — pick a fixture, see the view, collect a decision, read what
 * the view posts. Files are watched; a change reloads the view.
 *
 *   wicket-plugin dev ./plugins/artifact [--port 4790] [--no-open]
 *
 * Node, and the icon set the package depends on; nothing else.
 */
import { execFile } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { iconsDir, packageRoot } from "../lib/paths.cjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = packageRoot(fileURLToPath(import.meta.url));
const sdkSrc = path.join(root, "src");
// A plugin's icons simply do not render when no set is found.
const iconDir = iconsDir(root);

const mime = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".gif": "image/gif",
  ".webp": "image/webp",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
  ".ttf": "font/ttf",
  ".map": "application/json",
  ".txt": "text/plain; charset=utf-8",
  ".md": "text/markdown; charset=utf-8",
};

const csp = (origin) =>
  [
    "default-src 'none'",
    `script-src 'unsafe-inline' ${origin}/plugin/ ${origin}/sdk/`,
    `style-src 'unsafe-inline' ${origin}/plugin/ ${origin}/sdk/`,
    `img-src data: blob: ${origin}/plugin/ ${origin}/sdk/`,
    `font-src data: ${origin}/plugin/`,
    `media-src data: blob: ${origin}/plugin/`,
    "connect-src 'none'",
    "form-action 'none'",
    "base-uri 'none'",
    "frame-ancestors 'self'",
  ].join("; ");

/** A file under `root`, or null when the path escapes it or is not a file. */
function under(root, rel) {
  const file = path.resolve(root, decodeURIComponent(rel).replace(/^\/+/, ""));
  if (!file.startsWith(path.resolve(root) + path.sep) && file !== path.resolve(root)) return null;
  return fs.existsSync(file) && fs.statSync(file).isFile() ? file : null;
}

function send(res, status, body, headers = {}) {
  res.writeHead(status, { "cache-control": "no-cache", ...headers });
  res.end(body);
}

function sendFile(res, file, headers = {}) {
  send(res, 200, fs.readFileSync(file), {
    "content-type": mime[path.extname(file).toLowerCase()] ?? "application/octet-stream",
    "x-content-type-options": "nosniff",
    ...headers,
  });
}

function fixtures(pluginDir) {
  const dir = path.join(pluginDir, "fixtures");
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir)
    .filter((f) => f.endsWith(".json"))
    .sort()
    .map((f) => {
      try {
        const g = JSON.parse(fs.readFileSync(path.join(dir, f), "utf8"));
        return { name: f, title: g.title ?? f, decided: !!g.decision };
      } catch {
        return { name: f, title: `${f} (invalid JSON)`, decided: false, broken: true };
      }
    });
}

/** The newest change under the plugin directory, so the page can reload. */
function stamp(pluginDir) {
  let latest = 0;
  const walk = (dir, depth) => {
    if (depth > 4) return;
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === "node_modules" || entry.name.startsWith(".")) continue;
      const p = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(p, depth + 1);
      else latest = Math.max(latest, fs.statSync(p).mtimeMs);
    }
  };
  try {
    walk(pluginDir, 0);
  } catch {
    // a file vanished mid-walk: the next poll sees the new state
  }
  return latest;
}

/**
 * Serves `dir` and opens the shell. `argv` is the command line after `dev`:
 * a directory (default "."), --port N, --no-open (or --open, the default).
 */
export function serve(argv) {
  const args = [...argv];
  const flag = (name) => {
    const i = args.indexOf(name);
    return i >= 0 ? args[i + 1] : undefined;
  };
  const pluginDir = path.resolve(args.find((a, i) => !a.startsWith("--") && args[i - 1] !== "--port") ?? ".");
  const port = Number(flag("--port") ?? 4790);
  const open = !args.includes("--no-open");

  if (!fs.existsSync(path.join(pluginDir, "manifest.json"))) {
    console.error(`no manifest.json in ${pluginDir}\nusage: wicket-plugin dev <plugin directory> [--port N] [--no-open]`);
    process.exit(2);
  }

  const server = http.createServer((req, res) => {
    const url = new URL(req.url, `http://${req.headers.host}`);
    const origin = `http://${req.headers.host}`;
    const p = url.pathname;

    if (p === "/") return sendFile(res, path.join(here, "shell.html"));
    if (p === "/dev/manifest") {
      try {
        return send(res, 200, fs.readFileSync(path.join(pluginDir, "manifest.json")), { "content-type": "application/json" });
      } catch (e) {
        return send(res, 500, JSON.stringify({ error: String(e) }), { "content-type": "application/json" });
      }
    }
    if (p === "/dev/fixtures") return send(res, 200, JSON.stringify(fixtures(pluginDir)), { "content-type": "application/json" });
    if (p.startsWith("/dev/fixtures/")) {
      const file = under(path.join(pluginDir, "fixtures"), p.slice("/dev/fixtures/".length));
      return file ? sendFile(res, file) : send(res, 404, "no such fixture");
    }
    if (p === "/dev/stamp") return send(res, 200, JSON.stringify({ stamp: stamp(pluginDir), dir: pluginDir, icons: !!iconDir }), { "content-type": "application/json" });
    if (p === "/sdk/v1/wicket-plugin.js" || p === "/sdk/v1/wicket-plugin.css") {
      return sendFile(res, path.join(sdkSrc, path.basename(p)), { "access-control-allow-origin": "*" });
    }
    if (p.startsWith("/sdk/v1/icons/")) {
      const file = iconDir && under(iconDir, path.basename(p));
      return file ? sendFile(res, file, { "access-control-allow-origin": "*" }) : send(res, 404, "no such icon");
    }
    if (p.startsWith("/plugin/")) {
      const file = under(pluginDir, p.slice("/plugin/".length));
      return file ? sendFile(res, file, { "content-security-policy": csp(origin), "access-control-allow-origin": "*" }) : send(res, 404, "not found");
    }
    return send(res, 404, "not found");
  });

  server.listen(port, "127.0.0.1", () => {
    const url = `http://127.0.0.1:${port}/`;
    const manifest = JSON.parse(fs.readFileSync(path.join(pluginDir, "manifest.json"), "utf8"));
    console.log(`${manifest.name ?? "plugin"} v${manifest.version ?? "?"} from ${pluginDir}`);
    console.log(`shell at ${url}${iconDir ? "" : "  (no icon set found; icons will not render)"}`);
    console.log(`in the app: wicket plugins install ${pluginDir} --link`);
    if (open) {
      const cmd = process.platform === "darwin" ? "open" : process.platform === "win32" ? "start" : "xdg-open";
      execFile(cmd, [url], () => {});
    }
  });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) serve(process.argv.slice(2));
