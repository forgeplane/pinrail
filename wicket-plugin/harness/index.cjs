/**
 * Mounts a plugin directory in a sandboxed iframe under a fake shell, with
 * the SDK served at /sdk/v1/wicket-plugin.js and the same CSP the app uses,
 * so a plugin can be tested alone: no wicket server, no CLI.
 *
 *   import { fixture, mountPlugin } from "wicket-plugin/testing";
 */
const fs = require("node:fs");
const path = require("node:path");
const { iconsDir: findIcons, packageRoot, sdkScript } = require("../lib/paths.cjs");

const ORIGIN = "http://plugin.test";
const root = packageRoot(__filename);
const iconsDir = findIcons(root);

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

function csp() {
  return [
    "default-src 'none'",
    `script-src 'unsafe-inline' ${ORIGIN}/`,
    `style-src 'unsafe-inline' ${ORIGIN}/`,
    `img-src data: blob: ${ORIGIN}/ ${ORIGIN}/sdk/`,
    `font-src data: ${ORIGIN}/`,
    `media-src data: blob: ${ORIGIN}/`,
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

/** A fixture file (`{ title, payload }`, or with a `decision`) as a gate. */
function fixture(file) {
  return gateFrom(JSON.parse(fs.readFileSync(file, "utf8")));
}

async function mountPlugin(page, pluginDir, opts) {
  const sdk = sdkScript(root);
  const sdkCss = fs.readFileSync(path.join(root, "src", "wicket-plugin.css"), "utf8");
  const harness = fs.readFileSync(path.join(__dirname, "harness.html"), "utf8");

  await page.route(`${ORIGIN}/**`, async (route) => {
    const url = new URL(route.request().url());
    const p = url.pathname;
    if (p === "/_harness.html") return route.fulfill({ contentType: "text/html", body: harness });
    if (p === "/sdk/v1/wicket-plugin.js") return route.fulfill({ contentType: mime[".js"], body: sdk });
    if (p === "/sdk/v1/wicket-plugin.css") return route.fulfill({ contentType: mime[".css"], body: sdkCss });
    // the stylesheet imports a typeface; tests run offline and in the system font
    if (p === "/sdk/v1/fonts.css") return route.fulfill({ contentType: mime[".css"], body: "" });
    if (p.startsWith("/sdk/v1/icons/")) {
      const icon = iconsDir && path.join(iconsDir, path.basename(p));
      if (!icon || !fs.existsSync(icon)) return route.fulfill({ status: 404, body: "no such icon" });
      // The app sends this because a view's frame has an opaque origin and a
      // mask image is fetched under CORS. A fulfilled route is exempt from that
      // check, so the header here only keeps the harness honest; whether the
      // real policy lets an icon through is settled by the end-to-end suite.
      return route.fulfill({
        contentType: mime[".svg"],
        body: fs.readFileSync(icon),
        headers: { "access-control-allow-origin": "*" },
      });
    }
    const file = path.join(pluginDir, decodeURIComponent(p.replace(/^\//, "")));
    if (!file.startsWith(path.resolve(pluginDir)) || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
      return route.fulfill({ status: 404, body: "not found" });
    }
    return route.fulfill({
      body: fs.readFileSync(file),
      contentType: mime[path.extname(file)] ?? "application/octet-stream",
      headers: { "content-security-policy": csp(), "x-content-type-options": "nosniff" },
    });
  });

  const manifest = JSON.parse(fs.readFileSync(path.join(pluginDir, "manifest.json"), "utf8"));
  await page.goto(`${ORIGIN}/_harness.html?theme=${opts.theme ?? "dark"}&entry=${encodeURIComponent(manifest.entry ?? "index.html")}`);
  const init = { gate: gateFrom(opts.gate), previous: opts.previous ?? null, readonly: !!opts.readonly, draft: opts.draft ?? null, settings: opts.settings ?? {} };
  await page.evaluate((i) => window.__shell.init(i), init);

  const messages = () => page.evaluate(() => window.__shell.messages());
  const send = (msg) => page.evaluate((m) => window.__shell.send(m), msg);

  return {
    frame: page.frameLocator("#plugin-frame"),
    messages,
    async nextSubmit(after = 0) {
      await page.waitForFunction((n) => window.__shell.messages().filter((m) => m.type === "submit").length > n, after);
      const all = await messages();
      return all.filter((m) => m.type === "submit").pop().data;
    },
    lastDraft: () => page.evaluate(() => window.__shell.lastDraft()),
    lastStatus: () => page.evaluate(() => window.__shell.lastStatus()),
    lastSettingsSet: () => page.evaluate(() => window.__shell.lastSettingsSet()),
    settings: (values) => page.evaluate((v) => window.__shell.settings(v), values),
    sendKey: (combo) => page.evaluate((c) => window.__shell.sendKey(c), combo),
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
