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

// The typeface the app draws plugin views in. The app bundles the files so it
// needs no network; this server is a preview on a developer's own machine, so
// it points at the same files on Fontsource's CDN rather than shipping 200 KB
// of fonts in the npm package. Offline, the panel falls back to the system
// font and nothing else changes.
const FONT_CDN = "https://cdn.jsdelivr.net";
const FONT_SUBSETS = [
  ["cyrillic-ext", "U+0460-052F,U+1C80-1C8A,U+20B4,U+2DE0-2DFF,U+A640-A69F,U+FE2E-FE2F"],
  ["cyrillic", "U+0301,U+0400-045F,U+0490-0491,U+04B0-04B1,U+2116"],
  ["greek-ext", "U+1F00-1FFF"],
  ["greek", "U+0370-0377,U+037A-037F,U+0384-038A,U+038C,U+038E-03A1,U+03A3-03FF"],
  ["vietnamese", "U+0102-0103,U+0110-0111,U+0128-0129,U+0168-0169,U+01A0-01A1,U+01AF-01B0,U+0300-0301,U+0303-0304,U+0308-0309,U+0323,U+0329,U+1EA0-1EF9,U+20AB"],
  ["latin-ext", "U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF"],
  ["latin", "U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD"],
];

const fontsCss = () =>
  FONT_SUBSETS.map(
    ([subset, range]) => `@font-face {
  font-family: 'Inter Variable';
  font-style: normal;
  font-display: swap;
  font-weight: 100 900;
  src: url(${FONT_CDN}/fontsource/fonts/inter:vf@latest/${subset}-wght-normal.woff2) format('woff2-variations');
  unicode-range: ${range};
}`,
  ).join("\n\n");

const csp = (origin) =>
  [
    "default-src 'none'",
    `script-src 'unsafe-inline' ${origin}/plugin/ ${origin}/sdk/`,
    `style-src 'unsafe-inline' ${origin}/plugin/ ${origin}/sdk/`,
    `img-src data: blob: ${origin}/plugin/ ${origin}/sdk/`,
    `font-src data: ${origin}/plugin/ ${FONT_CDN}`,
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
    if (p === "/sdk/v1/fonts.css") {
      return send(res, 200, fontsCss(), { "content-type": "text/css", "access-control-allow-origin": "*" });
    }
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
