#!/usr/bin/env node
/*
 * A shell for one plugin, in a browser, without the app: serves the plugin
 * directory under the app's CSP, the SDK beside it, and a page that plays
 * the shell — pick a fixture, see the view, collect a decision, read what
 * the view posts. Files are watched; a change reloads the view.
 *
 *   pinrail-plugin dev ./plugins/artifact [--port 4790] [--no-open]
 *
 * Node, and the icon set the package depends on; nothing else.
 */
import { execFile } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import Ajv2020 from "ajv/dist/2020.js";
import { resolveAttachments } from "../harness/attachments.cjs";
import { packageRoot, sdkScript } from "../lib/paths.cjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = packageRoot(fileURLToPath(import.meta.url));
const sdkSrc = path.join(root, "src");

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
  [
    "vietnamese",
    "U+0102-0103,U+0110-0111,U+0128-0129,U+0168-0169,U+01A0-01A1,U+01AF-01B0,U+0300-0301,U+0303-0304,U+0308-0309,U+0323,U+0329,U+1EA0-1EF9,U+20AB",
  ],
  [
    "latin-ext",
    "U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF",
  ],
  [
    "latin",
    "U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD",
  ],
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

/**
 * What the app would refuse in `data`, as `violations` errors: a decision
 * against the plugin's decision schema, or a change to its settings against
 * its settings schema, with the paths and wording the app uses.
 */
function violations(pluginDir, kind, data) {
  const manifest = JSON.parse(fs.readFileSync(path.join(pluginDir, "manifest.json"), "utf8"));
  const load = (schema) =>
    schema && typeof schema.$ref === "string"
      ? JSON.parse(fs.readFileSync(path.join(pluginDir, schema.$ref), "utf8"))
      : schema;
  // the app keeps a plugin's settings under its full name, `/` written `~1`
  const prefix = kind === "settings" ? `/plugins/local~1${manifest.name}` : "";
  let schema = load(
    kind === "settings" ? manifest.settings_schema : { $ref: path.join("schemas", "decision.schema.json") },
  );
  if (!schema) return kind === "settings" ? [{ path: prefix, message: "the plugin has no settings" }] : [];
  schema = { ...schema };
  delete schema.$schema;
  delete schema.$id;
  // the app refuses a setting the schema does not declare
  if (kind === "settings") schema = { ...schema, type: "object", additionalProperties: false };
  const validate = new Ajv2020({ allErrors: true, strict: false, validateFormats: false }).compile(schema);
  if (validate(data)) return [];
  return validate.errors.map((e) => {
    const at =
      e.keyword === "additionalProperties" ? `${e.instancePath}/${e.params.additionalProperty}` : e.instancePath;
    // a property with choices names them, as the app does
    const message = e.keyword === "enum" ? `must be one of ${e.params.allowedValues.join(", ")}` : e.message;
    return { path: prefix + at, message };
  });
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
  const usage = "usage: pinrail-plugin dev <plugin directory> [--port N] [--no-open]";
  let parsed;
  try {
    parsed = parseArgs({
      args: argv,
      allowPositionals: true,
      options: { port: { type: "string" }, "no-open": { type: "boolean" } },
    });
  } catch (error) {
    console.error(`pinrail-plugin dev: ${error.message}\n${usage}`);
    process.exit(2);
  }
  const pluginDir = path.resolve(parsed.positionals[0] ?? ".");
  const port = Number(parsed.values.port ?? 4790);
  const open = !parsed.values["no-open"];

  if (!fs.existsSync(path.join(pluginDir, "manifest.json"))) {
    console.error(`no manifest.json in ${pluginDir}\n${usage}`);
    process.exit(2);
  }

  const server = http.createServer((req, res) => {
    const url = new URL(req.url, `http://${req.headers.host}`);
    const origin = `http://${req.headers.host}`;
    const p = url.pathname;

    if (p === "/") return sendFile(res, path.join(here, "shell.html"));
    // the app's side of the protocol, which the shell page runs
    if (p === "/dev/host.js") return sendFile(res, path.join(root, "host", "host.js"));
    if (p === "/dev/manifest") {
      try {
        return send(res, 200, fs.readFileSync(path.join(pluginDir, "manifest.json")), {
          "content-type": "application/json",
        });
      } catch (e) {
        return send(res, 500, JSON.stringify({ error: String(e) }), { "content-type": "application/json" });
      }
    }
    if (p === "/dev/fixtures")
      return send(res, 200, JSON.stringify(fixtures(pluginDir)), { "content-type": "application/json" });
    if (p.startsWith("/dev/fixtures/")) {
      const file = under(path.join(pluginDir, "fixtures"), p.slice("/dev/fixtures/".length));
      if (!file) return send(res, 404, "no such fixture");
      // the files a fixture lists by path, as the app lists them: name, size, type, hash
      try {
        const fixture = JSON.parse(fs.readFileSync(file, "utf8"));
        if (fixture.attachments && !Array.isArray(fixture.attachments))
          fixture.attachments = resolveAttachments(fixture.attachments, path.dirname(file)).list;
        return send(res, 200, JSON.stringify(fixture), { "content-type": "application/json" });
      } catch (e) {
        return send(res, 422, JSON.stringify({ error: String(e.message || e) }), {
          "content-type": "application/json",
        });
      }
    }
    // a file a fixture carries, fetched by the shell for the view that asked
    if (p.startsWith("/dev/attachments/")) {
      const [fixtureName, ...rest] = p.slice("/dev/attachments/".length).split("/").map(decodeURIComponent);
      const file = under(path.join(pluginDir, "fixtures"), fixtureName);
      try {
        const fixture = file && JSON.parse(fs.readFileSync(file, "utf8"));
        const entry = fixture && resolveAttachments(fixture.attachments, path.dirname(file)).files[rest.join("/")];
        return entry
          ? sendFile(res, entry.path, { "content-type": "application/octet-stream" })
          : send(res, 404, "no such attachment");
      } catch {
        return send(res, 404, "no such attachment");
      }
    }
    // what the app would refuse in a decision or a change to the settings
    if (p === "/dev/check" && req.method === "POST") {
      let body = "";
      req.on("data", (chunk) => (body += chunk));
      req.on("end", () => {
        try {
          const { kind, data } = JSON.parse(body);
          send(res, 200, JSON.stringify({ errors: violations(pluginDir, kind, data) }), {
            "content-type": "application/json",
          });
        } catch (e) {
          send(res, 400, JSON.stringify({ error: String(e.message || e) }), { "content-type": "application/json" });
        }
      });
      return;
    }
    if (p === "/dev/stamp")
      return send(res, 200, JSON.stringify({ stamp: stamp(pluginDir), dir: pluginDir }), {
        "content-type": "application/json",
      });
    if (p === "/sdk/v1/fonts.css") {
      return send(res, 200, fontsCss(), { "content-type": "text/css", "access-control-allow-origin": "*" });
    }
    // the SDK carries its markdown parser, as it does in the app
    if (p === "/sdk/v1/pinrail-plugin.js") {
      return send(res, 200, sdkScript(root), { "content-type": mime[".js"], "access-control-allow-origin": "*" });
    }
    if (p === "/sdk/v1/pinrail-plugin.css" || p === "/sdk/v1/tokens.css")
      return sendFile(res, path.join(sdkSrc, path.basename(p)), { "access-control-allow-origin": "*" });
    if (p.startsWith("/plugin/")) {
      const file = under(pluginDir, p.slice("/plugin/".length));
      return file
        ? sendFile(res, file, { "content-security-policy": csp(origin), "access-control-allow-origin": "*" })
        : send(res, 404, "not found");
    }
    return send(res, 404, "not found");
  });

  server.listen(port, "127.0.0.1", () => {
    const url = `http://127.0.0.1:${port}/`;
    const manifest = JSON.parse(fs.readFileSync(path.join(pluginDir, "manifest.json"), "utf8"));
    console.log(`${manifest.name ?? "plugin"} v${manifest.version ?? "?"} from ${pluginDir}`);
    console.log(`shell at ${url}`);
    console.log(`in the app: pinrail plugins install ${pluginDir} --link`);
    if (open) {
      const cmd = process.platform === "darwin" ? "open" : process.platform === "win32" ? "start" : "xdg-open";
      execFile(cmd, [url], () => {});
    }
  });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) serve(process.argv.slice(2));
