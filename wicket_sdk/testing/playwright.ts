/**
 * Mounts a plugin directory in a sandboxed iframe under a fake shell, with
 * the SDK served at /sdk/v1/wicket-plugin.js and the same CSP the app uses,
 * so a plugin can be tested alone: no wicket server, no CLI.
 */
import type { Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";

const sdkRoot = path.resolve(__dirname, "..");
const ORIGIN = "http://plugin.test";

const mime: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff2": "font/woff2",
};

function csp(): string {
  return [
    "default-src 'none'",
    `script-src 'unsafe-inline' ${ORIGIN}/`,
    `style-src 'unsafe-inline' ${ORIGIN}/`,
    `img-src data: blob: ${ORIGIN}/`,
    `font-src data: ${ORIGIN}/`,
    `media-src data: blob: ${ORIGIN}/`,
    "connect-src 'none'",
    "form-action 'none'",
    "base-uri 'none'",
  ].join("; ");
}

export type Gate = Record<string, any>;

export type MountOptions = {
  gate: Gate;
  previous?: Gate | null;
  readonly?: boolean;
  draft?: any;
};

export type Message = { wicket: 1; type: string; [k: string]: any };

export type MountedPlugin = {
  frame: ReturnType<Page["frameLocator"]>;
  messages(): Promise<Message[]>;
  /** waits for the next `submit` after `after` messages had been seen */
  nextSubmit(after?: number): Promise<any>;
  lastDraft(): Promise<any>;
  send(msg: Record<string, any>): Promise<void>;
  sendViolations(errors: { path: string; message: string }[]): Promise<void>;
  sendSubmitted(decision: Gate): Promise<void>;
  collect(): Promise<void>;
  /** re-sends init with the last draft, as the shell does after a reload */
  reinit(overrides?: Partial<MountOptions>): Promise<void>;
  reload(): Promise<void>;
};

/** A gate envelope with defaults, from a fixture's partial gate. */
export function gateFrom(partial: Gate): Gate {
  return {
    id: "g_test",
    type: "test",
    type_version: 1,
    title: "test gate",
    source: { repo: "acme", workflow: "test" },
    requested_by: "test",
    created_at: "2026-09-11T10:00:00Z",
    expires_at: null,
    supersedes: null,
    summary: null,
    status: partial.decision ? "decided" : "pending",
    decision: null,
    agent_note: null,
    ...partial,
  };
}

export function fixture(file: string): Gate {
  return gateFrom(JSON.parse(fs.readFileSync(file, "utf8")));
}

export async function mountPlugin(page: Page, pluginDir: string, opts: MountOptions): Promise<MountedPlugin> {
  const sdk = fs.readFileSync(path.join(sdkRoot, "src", "wicket-plugin.js"), "utf8");
  const harness = fs.readFileSync(path.join(sdkRoot, "testing", "harness.html"), "utf8");

  await page.route(`${ORIGIN}/**`, async (route) => {
    const url = new URL(route.request().url());
    const p = url.pathname;
    if (p === "/_harness.html") return route.fulfill({ contentType: "text/html", body: harness });
    if (p === "/sdk/v1/wicket-plugin.js") return route.fulfill({ contentType: mime[".js"], body: sdk });
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

  await page.goto(`${ORIGIN}/_harness.html`);
  const init = { gate: gateFrom(opts.gate), previous: opts.previous ?? null, readonly: !!opts.readonly, draft: opts.draft ?? null };
  await page.evaluate((i) => (window as any).__shell.init(i), init);

  const messages = () => page.evaluate(() => (window as any).__shell.messages() as Message[]);
  const send = (msg: Record<string, any>) => page.evaluate((m) => (window as any).__shell.send(m), msg);

  return {
    frame: page.frameLocator("#plugin-frame"),
    messages,
    async nextSubmit(after = 0) {
      await page.waitForFunction((n) => (window as any).__shell.messages().filter((m: Message) => m.type === "submit").length > n, after);
      const all = await messages();
      return all.filter((m) => m.type === "submit").pop()!.data;
    },
    lastDraft: () => page.evaluate(() => (window as any).__shell.lastDraft()),
    send,
    sendViolations: (errors) => send({ type: "violations", errors }),
    sendSubmitted: (decision) => send({ type: "submitted", decision }),
    collect: () => send({ type: "collect" }),
    reinit: (overrides = {}) => page.evaluate((o) => (window as any).__shell.reinit(o), overrides as any),
    reload: () => page.evaluate(() => (window as any).__shell.reload()),
  };
}
