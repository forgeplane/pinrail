#!/usr/bin/env node
/*
 * A shell for one plugin, in a browser, without the app: serves the plugin
 * directory under the app's CSP, the SDK beside it, and a page that plays
 * the shell — pick a fixture, see the view, collect a decision, read what
 * the view posts, and select parts of the view to comment on. Files are
 * watched; a change reloads the view. A POST to /dev/clear-comments clears
 * the page's comments, for an agent that has made the changes they asked.
 *
 *   pinrail-sdk dev ./plugins/hello [--port 4790] [--no-open]
 *
 * Node, and the icon set the package depends on; nothing else.
 */
import { execFile } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import Ajv2020 from "ajv/dist/2020.js";
import { resolveAttachments } from "../harness/attachments.cjs";
import { packageRoot, markdownScript } from "../lib/paths.cjs";

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

// The typeface the app draws plugin views in, from the same package the app
// bundles, so a view looks as it does in the app and needs no network.
const fonts = path.dirname(createRequire(import.meta.url).resolve("@fontsource-variable/inter/wght.css"));

const csp = (origin) =>
  [
    "default-src 'none'",
    `script-src 'unsafe-inline' ${origin}/plugin/ ${origin}/sdk/`,
    `style-src 'unsafe-inline' ${origin}/plugin/ ${origin}/sdk/`,
    `img-src data: blob: ${origin}/plugin/ ${origin}/sdk/`,
    `font-src data: ${origin}/plugin/ ${origin}/sdk/`,
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

/**
 * The view's pages, with the inspector the shell's Select drives added at
 * their end, inline, since the view's CSP allows inline scripts and none
 * from the shell's own paths.
 */
const inspector = fs.readFileSync(path.join(here, "inspector.js"), "utf8");
const withInspector = (html) => {
  const tag = `<script data-pinrail-review>${inspector}</script>`;
  return /<\/body>/i.test(html) ? html.replace(/<\/body>(?![\s\S]*<\/body>)/i, tag + "</body>") : html + tag;
};

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

/** The folders whose reviews the shell offers: the samples the app sends,
 *  then the fixtures kept for development and tests. */
const REVIEW_DIRS = ["samples", "fixtures"];

/** The reviews the shell offers, named by their path in the plugin folder. */
function fixtures(pluginDir) {
  return REVIEW_DIRS.flatMap((sub) => {
    const dir = path.join(pluginDir, sub);
    if (!fs.existsSync(dir)) return [];
    return (
      fs
        .readdirSync(dir)
        // a decided fixture's summary, recorded beside it, is not a review
        .filter((f) => f.endsWith(".json") && !f.endsWith(".summary.json"))
        .sort()
        .map((f) => {
          const name = `${sub}/${f}`;
          try {
            const g = JSON.parse(fs.readFileSync(path.join(dir, f), "utf8"));
            return { name, title: g.title ?? name, decided: !!g.decision };
          } catch {
            return { name, title: `${name} (invalid JSON)`, decided: false, broken: true };
          }
        })
    );
  });
}

/** The file of a review the shell offers, by its name, or null. */
function fixtureFile(pluginDir, name) {
  const sub = REVIEW_DIRS.find((d) => decodeURIComponent(name).startsWith(`${d}/`));
  return sub ? under(path.join(pluginDir, sub), decodeURIComponent(name).slice(sub.length + 1)) : null;
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
  const usage = "usage: pinrail-sdk dev <plugin directory> [--port N] [--no-open]";
  let parsed;
  try {
    parsed = parseArgs({
      args: argv,
      allowPositionals: true,
      options: { port: { type: "string" }, "no-open": { type: "boolean" } },
    });
  } catch (error) {
    console.error(`pinrail-sdk dev: ${error.message}\n${usage}`);
    process.exit(2);
  }
  const pluginDir = path.resolve(parsed.positionals[0] ?? ".");
  const port = Number(parsed.values.port ?? 4790);
  const open = !parsed.values["no-open"];

  if (!fs.existsSync(path.join(pluginDir, "manifest.json"))) {
    console.error(`no manifest.json in ${pluginDir}\n${usage}`);
    process.exit(2);
  }

  // when the comments were last cleared from outside the page, by an agent
  // that has made the changes they asked for; the page drops every comment
  // made before then, whenever it hears of it
  let commentsCleared = 0;

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
    // the schemas the payload and the decision are held to
    const schema = /^\/dev\/schemas\/(payload|decision)$/.exec(p);
    if (schema) {
      try {
        const text = fs.readFileSync(path.join(pluginDir, "schemas", `${schema[1]}.schema.json`));
        return send(res, 200, text, { "content-type": "application/json" });
      } catch {
        return send(res, 404, JSON.stringify({ error: `no schemas/${schema[1]}.schema.json` }), {
          "content-type": "application/json",
        });
      }
    }
    if (p === "/dev/fixtures")
      return send(res, 200, JSON.stringify(fixtures(pluginDir)), { "content-type": "application/json" });
    if (p.startsWith("/dev/fixtures/")) {
      const file = fixtureFile(pluginDir, p.slice("/dev/fixtures/".length));
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
      const file = fixtureFile(pluginDir, fixtureName);
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
    if (p === "/dev/clear-comments" && req.method === "POST") {
      commentsCleared = Date.now();
      return send(res, 204, "");
    }
    if (p === "/dev/stamp")
      return send(res, 200, JSON.stringify({ stamp: stamp(pluginDir), dir: pluginDir, commentsCleared }), {
        "content-type": "application/json",
      });
    // the stylesheet imports ./fonts.css, which names its faces in ./files/
    if (p === "/sdk/v1/fonts.css")
      return sendFile(res, path.join(fonts, "wght.css"), { "access-control-allow-origin": "*" });
    if (p.startsWith("/sdk/v1/files/")) {
      const file = under(path.join(fonts, "files"), p.slice("/sdk/v1/files/".length));
      return file ? sendFile(res, file, { "access-control-allow-origin": "*" }) : send(res, 404, "not found");
    }
    // the Markdown renderer, with its parser in front of it, as in the app
    if (p === "/sdk/v1/markdown.js") {
      return send(res, 200, markdownScript(root), { "content-type": mime[".js"], "access-control-allow-origin": "*" });
    }
    if (p === "/sdk/v1/pinrail-plugin.js" || p === "/sdk/v1/pinrail-plugin.css" || p === "/sdk/v1/tokens.css")
      return sendFile(res, path.join(sdkSrc, path.basename(p)), { "access-control-allow-origin": "*" });
    if (p.startsWith("/plugin/")) {
      const file = under(pluginDir, p.slice("/plugin/".length));
      if (!file) return send(res, 404, "not found");
      const headers = { "content-security-policy": csp(origin), "access-control-allow-origin": "*" };
      if (path.extname(file).toLowerCase() === ".html") {
        return send(res, 200, withInspector(fs.readFileSync(file, "utf8")), {
          "content-type": mime[".html"],
          "x-content-type-options": "nosniff",
          ...headers,
        });
      }
      return sendFile(res, file, headers);
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
