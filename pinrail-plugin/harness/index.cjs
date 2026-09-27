/**
 * Mounts a plugin directory in a sandboxed iframe under a fake shell, with
 * the SDK served at /sdk/v1/pinrail-plugin.js and the same CSP the app uses,
 * so a plugin can be tested alone: no pinrail server, no CLI.
 *
 *   import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";
 */
const fs = require("node:fs");
const path = require("node:path");
const { packageRoot, sdkScript } = require("../lib/paths.cjs");
const { resolveAttachments } = require("./attachments.cjs");
const Ajv2020 = require("ajv/dist/2020").default;

const ORIGIN = "http://plugin.test";

const MAC = process.platform === "darwin";
const MODIFIER = { cmd: "cmd", command: "cmd", meta: "cmd", super: "cmd", ctrl: "ctrl", control: "ctrl", alt: "alt", option: "alt", shift: "shift", cmdorctrl: MAC ? "cmd" : "ctrl", commandorcontrol: MAC ? "cmd" : "ctrl" };

/** A combination as the app compares it: modifiers by one name, in the
 *  order ctrl, alt, shift, cmd, then the key. The same as the core's. */
function normalizeKeys(keys) {
  const parts = String(keys).split("+").map((p) => p.trim().toLowerCase());
  const key = parts.pop();
  const named = parts.map((m) => MODIFIER[m] || m);
  return [...["ctrl", "alt", "shift", "cmd"].filter((m) => named.includes(m)), key].join("+");
}

/** The keys the app keeps for itself, and so never forwards to a view; the
 *  same as APP_KEYS in desktop/app/ui/src/lib/shortcuts.ts. */
const PRIMARY = MAC ? "cmd" : "ctrl";
const APP_KEYS = new Set([
  ...["k", ",", "i", "b", "[", "]", "q", "w", "m", "z", "x", "c", "v", "a"].map((k) => normalizeKeys(`${PRIMARY}+${k}`)),
  ...["h", "p", "m", "l", "z"].map((k) => normalizeKeys(`${PRIMARY}+shift+${k}`)),
  ...(MAC ? ["cmd+h", "alt+cmd+h", "ctrl+cmd+f"] : []),
  "shift+/",
  "[",
  "]",
  "escape",
  "cmd+enter",
  "ctrl+enter",
]);
const appKeeps = (combo) => APP_KEYS.has(combo);
const root = packageRoot(__filename);

const mime = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".webp": "image/webp",
  ".woff2": "font/woff2",
};

/** The policy the app serves a plugin's files with (desktop/core/src/api/files.rs):
 *  the bundle's own path and the SDK's, and nothing else. */
function csp(bundle) {
  const own = `${ORIGIN}${bundle}`;
  const sdk = `${ORIGIN}/sdk/`;
  return [
    "sandbox allow-scripts",
    "default-src 'none'",
    `script-src 'unsafe-inline' ${own} ${sdk}`,
    `style-src 'unsafe-inline' ${own} ${sdk}`,
    `img-src data: blob: ${own} ${sdk}`,
    `font-src data: ${own} ${sdk}`,
    `media-src data: blob: ${own}`,
    "connect-src 'none'",
    "form-action 'none'",
    "base-uri 'none'",
  ].join("; ");
}

/** A gate envelope with defaults, from a fixture's partial gate: what the app hands a view. */
function gateFrom(partial) {
  return {
    id: "g_test",
    plugin: "test",
    plugin_version: 1,
    title: "test gate",
    origin: { repo: "acme", workflow: "test" },
    requested_by: "test",
    created_at: "2026-09-11T10:00:00Z",
    expires_at: null,
    revises: null,
    summary: null,
    status: partial.decision ? "decided" : "pending",
    decision: null,
    agent_note: null,
    payload: {},
    ...partial,
  };
}

/* The files behind a gate that fixture() read, for mountPlugin to serve:
   kept beside the gate rather than on it, since the gate goes to the view. */
const attachmentFiles = new WeakMap();

/** A fixture file (`{ title, payload }`, or with a `decision`, and with
 *  `attachments` by path) as a gate. */
function fixture(file) {
  const partial = JSON.parse(fs.readFileSync(file, "utf8"));
  const { list, files } = resolveAttachments(partial.attachments, path.dirname(file));
  const gate = gateFrom({ ...partial, attachments: list });
  attachmentFiles.set(gate, files);
  return gate;
}

/** Checks a decision against the plugin's decision_schema, inline or a
 *  file by `$ref`, as the core does: the first problem, or null. */
function decisionChecker(pluginDir, schema) {
  if (!schema) return () => null;
  let doc = schema;
  if (typeof schema.$ref === "string") doc = JSON.parse(fs.readFileSync(path.join(pluginDir, schema.$ref), "utf8"));
  doc = { ...doc };
  delete doc.$schema;
  delete doc.$id;
  const validate = new Ajv2020({ allErrors: false, strict: false, validateFormats: false }).compile(doc);
  return (data) => (validate(data) ? null : `${validate.errors[0].instancePath || "/"}: ${validate.errors[0].message}`);
}

async function mountPlugin(page, pluginDir, opts) {
  const sdk = sdkScript(root);
  const sdkCss = fs.readFileSync(path.join(root, "src", "pinrail-plugin.css"), "utf8");
  const harness = fs.readFileSync(path.join(__dirname, "harness.html"), "utf8");

  // the files the view may ask for: from fixture(), or given as { name: path }
  const given = opts.attachments ? resolveAttachments(opts.attachments, pluginDir) : null;
  const gate = gateFrom({ ...opts.gate, ...(given ? { attachments: given.list } : {}) });
  const previous = opts.previous ? gateFrom(opts.previous) : null;
  const served = {
    current: (given && given.files) || attachmentFiles.get(opts.gate) || {},
    previous: (opts.previous && attachmentFiles.get(opts.previous)) || {},
  };

  const manifest = JSON.parse(fs.readFileSync(path.join(pluginDir, "manifest.json"), "utf8"));
  const checkDecision = decisionChecker(pluginDir, manifest.decision_schema);
  // where the app serves it: /plugins/<name>/<major>/, a major of 1 here
  const bundle = `/plugins/${manifest.name}/1/`;
  await page.route(`${ORIGIN}/**`, async (route) => {
    const url = new URL(route.request().url());
    const p = url.pathname;
    if (p === "/_harness.html") return route.fulfill({ contentType: "text/html", body: harness });
    // the shell's own fetch of a file, as the app fetches it from the core
    if (p.startsWith("/_attachments/")) {
      const [round, ...rest] = p.slice("/_attachments/".length).split("/");
      const entry = (served[round] || {})[decodeURIComponent(rest.join("/"))];
      if (!entry) return route.fulfill({ status: 404, body: "no such attachment" });
      return route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(entry.path) });
    }
    if (p === "/sdk/v1/pinrail-plugin.js") return route.fulfill({ contentType: mime[".js"], body: sdk });
    if (p === "/sdk/v1/pinrail-plugin.css") return route.fulfill({ contentType: mime[".css"], body: sdkCss });
    // the stylesheet imports a typeface; tests run offline and in the system font
    if (p === "/sdk/v1/fonts.css") return route.fulfill({ contentType: mime[".css"], body: "" });
    if (!p.startsWith(bundle)) return route.fulfill({ status: 404, body: "not found" });
    const file = path.join(pluginDir, decodeURIComponent(p.slice(bundle.length)));
    if (!file.startsWith(path.resolve(pluginDir)) || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
      return route.fulfill({ status: 404, body: "not found" });
    }
    return route.fulfill({
      body: fs.readFileSync(file),
      contentType: mime[path.extname(file)] ?? "application/octet-stream",
      headers: { "content-security-policy": csp(bundle), "x-content-type-options": "nosniff" },
    });
  });

  // a view a build writes is not there until it runs: say so, rather than
  // time out on a frame that got a 404
  const entryFile = path.join(pluginDir, manifest.entry ?? "index.html");
  if (!fs.existsSync(entryFile)) {
    throw new Error(`${entryFile} does not exist${manifest.build ? `: build the plugin first (${manifest.build.command})` : ""}`);
  }
  await page.goto(`${ORIGIN}/_harness.html?theme=${opts.theme ?? "dark"}&entry=${encodeURIComponent(bundle + (manifest.entry ?? "index.html"))}`);
  const init = {
    gate,
    previous,
    readonly: !!opts.readonly,
    draft: opts.draft ?? null,
    settings: opts.settings ?? {},
    capabilities: opts.capabilities ?? ["attachments"],
  };
  await page.evaluate((i) => window.__shell.init(i), init);

  const messages = () => page.evaluate(() => window.__shell.messages());
  const send = (msg) => page.evaluate((m) => window.__shell.send(m), msg);

  return {
    frame: page.frameLocator("#plugin-frame"),
    messages,
    async nextSubmit(after = 0, { valid = true } = {}) {
      await page.waitForFunction((n) => window.__shell.messages().filter((m) => m.type === "submit").length > n, after);
      const all = await messages();
      const data = all.filter((m) => m.type === "submit").pop().data;
      // what the app would refuse, as the core checks every hand-over
      if (valid) {
        const wrong = checkDecision(data);
        if (wrong) throw new Error(`the decision does not pass decision_schema: ${wrong}\n${JSON.stringify(data)}`);
      }
      return data;
    },
    lastDraft: () => page.evaluate(() => window.__shell.lastDraft()),
    lastStatus: () => page.evaluate(() => window.__shell.lastStatus()),
    lastOpen: () => page.evaluate(() => window.__shell.lastOpen()),
    lastSettingsSet: () => page.evaluate(() => window.__shell.lastSettingsSet()),
    settings: (values) => page.evaluate((v) => window.__shell.settings(v), values),
    // what the app does with a key pressed while the shell has focus: it
    // forwards a combination the manifest declares, and not one it keeps
    sendKey: async (keys) => {
      const combo = normalizeKeys(keys);
      if (!(manifest.shortcuts || []).some((s) => normalizeKeys(s.keys) === combo)) {
        throw new Error(`${combo} is not declared in the manifest's shortcuts, so the app never forwards it`);
      }
      if (appKeeps(combo)) throw new Error(`${combo}: the app keeps this key for itself and never forwards it`);
      return page.evaluate((c) => window.__shell.sendKey(c), combo);
    },
    send,
    sendViolations: (errors) => send({ type: "violations", errors }),
    sendSubmitted: (decision) => send({ type: "submitted", decision }),
    collect: () => send({ type: "collect" }),
    setFrameHeight: (px) => page.evaluate((h) => window.__shell.setFrameHeight(h), px),
    reinit: (overrides = {}) => page.evaluate((o) => window.__shell.reinit(o), overrides),
    reload: () => page.evaluate(() => window.__shell.reload()),
  };
}

module.exports = { gateFrom, fixture, mountPlugin };
