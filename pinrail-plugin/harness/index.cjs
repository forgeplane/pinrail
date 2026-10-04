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
const MODIFIER = {
  cmd: "cmd",
  command: "cmd",
  meta: "cmd",
  super: "cmd",
  ctrl: "ctrl",
  control: "ctrl",
  alt: "alt",
  option: "alt",
  shift: "shift",
  cmdorctrl: MAC ? "cmd" : "ctrl",
  commandorcontrol: MAC ? "cmd" : "ctrl",
};

/** A combination as the app compares it: modifiers by one name, in the
 *  order ctrl, alt, shift, cmd, then the key. The same as the core's. */
function normalizeKeys(keys) {
  const parts = String(keys)
    .split("+")
    .map((p) => p.trim().toLowerCase());
  const key = parts.pop();
  const named = parts.map((m) => MODIFIER[m] || m);
  return [...["ctrl", "alt", "shift", "cmd"].filter((m) => named.includes(m)), key].join("+");
}

/** The keys the app keeps for itself, and so never forwards to a view; the
 *  same as APP_KEYS in desktop/app/ui/src/lib/shortcuts.ts. */
const PRIMARY = MAC ? "cmd" : "ctrl";
const APP_KEYS = new Set([
  ...["k", ",", "i", "b", "[", "]", "q", "w", "m", "z", "x", "c", "v", "a"].map((k) =>
    normalizeKeys(`${PRIMARY}+${k}`),
  ),
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

/** A review envelope with defaults, from a fixture's partial review: what the app hands a view. */
function reviewFrom(partial) {
  return {
    id: "g_test",
    plugin: "test",
    plugin_version: "1.0.0",
    plugin_bundle: null,
    title: "test review",
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

/* The files behind a review that fixture() read, for mountPlugin to serve:
   kept beside the review rather than on it, since the review goes to the view. */
const attachmentFiles = new WeakMap();

/** A fixture file (`{ title, payload }`, or with a `decision`, and with
 *  `attachments` by path) as a review. */
function fixture(file) {
  const partial = JSON.parse(fs.readFileSync(file, "utf8"));
  const { list, files } = resolveAttachments(partial.attachments, path.dirname(file));
  const review = reviewFrom({ ...partial, attachments: list });
  attachmentFiles.set(review, files);
  return review;
}

/** Checks a decision against the plugin's decision schema,
 *  `schemas/decision.schema.json`, as the core does: the violations, as
 *  `{ path, message }`, or none. */
function decisionChecker(pluginDir) {
  const file = path.join(pluginDir, "schemas", "decision.schema.json");
  if (!fs.existsSync(file)) return () => [];
  const doc = { ...JSON.parse(fs.readFileSync(file, "utf8")) };
  delete doc.$schema;
  delete doc.$id;
  const validate = new Ajv2020({ allErrors: true, strict: false, validateFormats: false }).compile(doc);
  return (data) => (validate(data) ? [] : validate.errors.map((e) => ({ path: e.instancePath, message: e.message })));
}

/** Checks a change to the plugin's settings against the manifest's
 *  settings_schema, as the core does: the violations, or none. */
function settingsChecker(manifest) {
  const schema = manifest.settings_schema;
  if (!schema || typeof schema !== "object") {
    return () => [{ path: "", message: "the manifest declares no settings" }];
  }
  const doc = { ...schema };
  delete doc.$schema;
  delete doc.$id;
  // a change names some of the settings, not all of them
  delete doc.required;
  const validate = new Ajv2020({ allErrors: true, strict: false, validateFormats: false }).compile(doc);
  return (patch) => (validate(patch) ? [] : validate.errors.map((e) => ({ path: e.instancePath, message: e.message })));
}

/* The checkers of the plugin mounted last on each page: a page can expose a
   function only once, and a test may mount more than one plugin. */
const checkers = new WeakMap();

async function mountPlugin(page, pluginDir, opts) {
  const sdk = sdkScript(root);
  const sdkCss = fs.readFileSync(path.join(root, "src", "pinrail-plugin.css"), "utf8");
  const sdkTokens = fs.readFileSync(path.join(root, "src", "tokens.css"), "utf8");
  const harness = fs.readFileSync(path.join(__dirname, "harness.html"), "utf8");
  const host = fs.readFileSync(path.join(root, "host", "host.js"), "utf8");

  // the files the view may ask for: from fixture(), or given as { name: path }
  const given = opts.attachments ? resolveAttachments(opts.attachments, pluginDir) : null;
  const review = reviewFrom({ ...opts.review, ...(given ? { attachments: given.list } : {}) });
  const previous = opts.previous ? reviewFrom(opts.previous) : null;
  const served = {
    current: (given && given.files) || attachmentFiles.get(opts.review) || {},
    previous: (opts.previous && attachmentFiles.get(opts.previous)) || {},
  };

  const manifest = JSON.parse(fs.readFileSync(path.join(pluginDir, "manifest.json"), "utf8"));
  const checkDecision = decisionChecker(pluginDir);
  // where the app serves it: a bundle's view/, and nothing outside it
  const bundle = `/bundles/${manifest.name}/view/`;
  const viewDir = path.resolve(pluginDir, "view");
  await page.route(`${ORIGIN}/**`, async (route) => {
    const url = new URL(route.request().url());
    const p = url.pathname;
    if (p === "/_harness.html") return route.fulfill({ contentType: "text/html", body: harness });
    // the app's side of the protocol, which the harness page runs
    if (p === "/_host.js") return route.fulfill({ contentType: mime[".js"], body: host });
    // the shell's own fetch of a file, as the app fetches it from the core
    if (p.startsWith("/_attachments/")) {
      const [round, ...rest] = p.slice("/_attachments/".length).split("/");
      const entry = (served[round] || {})[decodeURIComponent(rest.join("/"))];
      if (!entry) return route.fulfill({ status: 404, body: "no such attachment" });
      return route.fulfill({ contentType: "application/octet-stream", body: fs.readFileSync(entry.path) });
    }
    if (p === "/sdk/v1/pinrail-plugin.js") return route.fulfill({ contentType: mime[".js"], body: sdk });
    if (p === "/sdk/v1/pinrail-plugin.css") return route.fulfill({ contentType: mime[".css"], body: sdkCss });
    if (p === "/sdk/v1/tokens.css") return route.fulfill({ contentType: mime[".css"], body: sdkTokens });
    // the stylesheet imports a typeface; tests run offline and in the system font
    if (p === "/sdk/v1/fonts.css") return route.fulfill({ contentType: mime[".css"], body: "" });
    if (!p.startsWith(bundle)) return route.fulfill({ status: 404, body: "not found" });
    const file = path.join(viewDir, decodeURIComponent(p.slice(bundle.length)));
    if (!file.startsWith(viewDir + path.sep) || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
      return route.fulfill({ status: 404, body: "not found" });
    }
    return route.fulfill({
      body: fs.readFileSync(file),
      contentType: mime[path.extname(file)] ?? "application/octet-stream",
      headers: { "content-security-policy": csp(bundle), "x-content-type-options": "nosniff" },
    });
  });

  // the hand-over checks a decision in Node, where the schema validator is
  if (!checkers.has(page)) {
    await page.exposeFunction("__pinrailCheckDecision", (data) => checkers.get(page).decision(data));
    await page.exposeFunction("__pinrailCheckSettings", (patch) => checkers.get(page).settings(patch));
  }
  checkers.set(page, { decision: checkDecision, settings: settingsChecker(manifest) });

  // a view a build writes is not there until it runs: say so, rather than
  // time out on a frame that got a 404
  const entryFile = path.join(pluginDir, "view", "index.html");
  if (!fs.existsSync(entryFile)) {
    throw new Error(`${entryFile} does not exist: build the plugin first`);
  }
  await page.goto(
    `${ORIGIN}/_harness.html?theme=${opts.theme ?? "dark"}&entry=${encodeURIComponent(bundle + "index.html")}`,
  );
  const init = {
    review,
    previous,
    readonly: !!opts.readonly,
    draft: opts.draft ?? null,
    settings: opts.settings ?? {},
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
        if (wrong.length) {
          const first = `${wrong[0].path || "/"}: ${wrong[0].message}`;
          throw new Error(`the decision does not pass decision_schema: ${first}\n${JSON.stringify(data)}`);
        }
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
    // the hand-over button: asks the view for its decision, as the app does
    collect: () => page.evaluate(() => window.__shell.collect()),
    // the whole hand-over, as the app does it: the decision accepted, the
    // violations of one the schema refuses, or the view's "not yet"
    handOver: () => page.evaluate(() => window.__shell.handOver()),
    setFrameHeight: (px) => page.evaluate((h) => window.__shell.setFrameHeight(h), px),
    reinit: (overrides = {}) => page.evaluate((o) => window.__shell.reinit(o), overrides),
    reload: () => page.evaluate(() => window.__shell.reload()),
  };
}

module.exports = { reviewFrom, fixture, mountPlugin };
