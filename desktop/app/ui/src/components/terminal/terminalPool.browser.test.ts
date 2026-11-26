// The pool of attached terminals with each renderer, against a stand-in
// for the core's attach socket.
import { afterEach, describe, expect, test } from "vitest";
import { RENDERERS } from "./renderers";
import type { SocketLike } from "./stream";
import { TerminalPool, type PoolDeps } from "./terminalPool";
import { referenceRenderer } from "./testing/reference";
import type { TerminalRenderer } from "./view";

const encoder = new TextEncoder();

/** The core's side of one attach: a hello, a snapshot, then output. */
class Attach implements SocketLike {
  binaryType = "";
  readyState = 1;
  onopen = null;
  onerror = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number; reason?: string }) => void) | null = null;
  sent: unknown[] = [];
  offset = 0;
  constructor(
    readonly url: URL,
    pty: { cols: number; rows: number },
    screen: string,
  ) {
    queueMicrotask(() => {
      const resume = url.searchParams.get("resume_from");
      this.json({ t: "hello", v: 1, role: "observer", cols: pty.cols, rows: pty.rows });
      if (resume) {
        this.offset = Number(resume);
        this.json({ t: "resume", offset: this.offset });
        return;
      }
      this.json({ t: "snapshot_begin", format: "vt", cols: pty.cols, rows: pty.rows, cut: 0 });
      this.binary(0x02, encoder.encode(screen));
      this.json({ t: "snapshot_end" });
    });
  }
  json(value: unknown) {
    this.onmessage?.({ data: JSON.stringify(value) });
  }
  binary(kind: number, payload: Uint8Array) {
    const head = kind === 0x01 ? 9 : 1;
    const message = new Uint8Array(head + payload.length);
    message[0] = kind;
    if (kind === 0x01) new DataView(message.buffer).setBigUint64(1, BigInt(this.offset), true);
    message.set(payload, head);
    if (kind === 0x01) this.offset += payload.length;
    this.onmessage?.({ data: message.buffer });
  }
  output(text: string) {
    this.binary(0x01, encoder.encode(text));
  }
  send(data: unknown) {
    this.sent.push(typeof data === "string" ? JSON.parse(data) : data);
  }
  close() {
    this.readyState = 3;
  }
  drop(code = 4010) {
    this.readyState = 3;
    this.onclose?.({ code, reason: "host offline" });
  }
}

const frame = () => new Promise((r) => requestAnimationFrame(() => r(null)));

function rig(renderer: TerminalRenderer, pty = { cols: 60, rows: 20 }) {
  const attaches: Attach[] = [];
  const deps: PoolDeps = {
    renderer: async () => renderer,
    appearance: () => ({
      theme: { foreground: "#ededef", background: "#18191b", cursor: "#ededef" },
      font: { family: "monospace", size: 13 },
      scheme: "dark",
    }),
    ticket: async (t) => `ticket-for-${t}`,
    url: async (t) => `ws://core/api/v1/terminals/${t}/attach`,
    open: (url) => {
      const terminal = decodeURIComponent(new URL(url).pathname.split("/")[4]);
      const attach = new Attach(new URL(url), pty, `screen of ${terminal}\r\n`);
      attaches.push(attach);
      return attach;
    },
  };
  // the app's stylesheet: the view fills the pane
  const style = document.createElement("style");
  style.textContent = ".terminal-surface { position: absolute; inset: 0; }";
  document.head.append(style);
  const pane = document.createElement("div");
  pane.style.cssText = "position: relative; width: 800px; height: 480px";
  document.body.append(pane);
  return { pool: new TerminalPool(deps), attaches, pane };
}

const panes: HTMLElement[] = [];
afterEach(() => {
  panes.splice(0).forEach((p) => p.remove());
  document.body.replaceChildren();
});

const adapters: [string, () => Promise<TerminalRenderer>][] = [
  ["reference", async () => referenceRenderer],
  ...Object.entries(RENDERERS),
];

for (const [name, make] of adapters) {
  describe(`terminal pool with ${name}`, () => {
    test("attaches at the size the pane fits and shows the snapshot", async () => {
      const { pool, attaches, pane } = rig(await make());
      const pooled = await pool.show("local:a", pane);
      await expect.poll(() => pooled.state.stream).toBe("live");
      const query = attaches[0].url.searchParams;
      const fit = pooled.view.measure();
      expect([query.get("cols"), query.get("rows")]).toEqual([String(fit.cols), String(fit.rows)]);
      await expect.poll(() => pooled.view.search("screen of local:a", {}).found).toBe(true);
      pool.clear();
    });

    test("switching back to a pooled terminal reuses its view within 50 ms", async () => {
      const { pool, attaches, pane } = rig(await make());
      const a = await pool.show("local:a", pane);
      await expect.poll(() => a.state.stream).toBe("live");
      pool.hide("local:a");
      const b = await pool.show("local:b", pane);
      await expect.poll(() => b.state.stream).toBe("live");
      pool.hide("local:b");
      await frame();
      attaches[0].output("while away\r\n");

      const started = performance.now();
      const again = await pool.show("local:a", pane);
      await frame();
      const took = performance.now() - started;

      expect(again).toBe(a);
      expect(attaches).toHaveLength(2);
      expect(pane.contains(a.element)).toBe(true);
      expect(pane.contains(b.element)).toBe(false);
      await expect.poll(() => a.view.search("while away", {}).found).toBe(true);
      expect(took).toBeLessThan(50);
      pool.clear();
    });

    test("the grid for an area is the one a view in that area measures", async () => {
      const renderer = await make();
      const { pool } = rig(renderer);
      const appearance = {
        theme: { foreground: "#ededef", background: "#18191b", cursor: "#ededef" },
        font: { family: "monospace", size: 13 },
      };
      for (const [width, height] of [
        [1000, 600],
        [333, 211],
      ]) {
        const host = document.createElement("div");
        host.style.cssText = `position: fixed; left: 0; top: 0; width: ${width}px; height: ${height}px`;
        document.body.append(host);
        const view = renderer.create();
        await view.mount(host, { cols: 80, rows: 24, scrollback: 0, ...appearance });
        const { cols, rows } = view.measure();
        view.dispose();
        host.remove();
        expect(await pool.gridFor(width, height)).toEqual({ cols, rows });
      }
    });

    test("keeps three terminals attached and reattaches the fourth one back", async () => {
      const { pool, attaches, pane } = rig(await make());
      for (const t of ["local:a", "local:b", "local:c", "local:d"]) {
        const pooled = await pool.show(t, pane);
        await expect.poll(() => pooled.state.stream).toBe("live");
        pool.hide(t);
      }
      expect(["a", "b", "c", "d"].map((t) => pool.has(`local:${t}`))).toEqual([false, true, true, true]);
      expect(attaches[0].readyState).toBe(3);
      const a = await pool.show("local:a", pane);
      await expect.poll(() => a.state.stream).toBe("live");
      expect(attaches).toHaveLength(5);
      expect(attaches[4].url.searchParams.has("resume_from")).toBe(false);
      pool.clear();
    });

    test("reconnects after a drop and goes on from where it stopped", async () => {
      const { pool, attaches, pane } = rig(await make());
      const a = await pool.show("local:a", pane);
      await expect.poll(() => a.state.stream).toBe("live");
      attaches[0].output("before\r\n");
      attaches[0].drop();
      await expect.poll(() => attaches.length, { timeout: 3000 }).toBe(2);
      await expect.poll(() => a.state.stream).toBe("live");
      expect(attaches[1].url.searchParams.get("resume_from")).toBe(String(attaches[0].offset));
      attaches[1].output("after\r\n");
      await expect.poll(() => a.view.search("after", {}).found).toBe(true);
      expect(a.view.search("before", {}).found).toBe(true);
      pool.clear();
    });

    test("an observer draws at the PTY's size and takes control with a click", async () => {
      const { pool, attaches, pane } = rig(await make(), { cols: 40, rows: 10 });
      const a = await pool.show("local:a", pane);
      await expect.poll(() => a.state.stream).toBe("live");
      expect(a.state.pty).toEqual({ cols: 40, rows: 10 });
      a.element.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
      expect(attaches[0].sent).toContainEqual({ t: "control", take: true });
      attaches[0].json({ t: "role", role: "controller" });
      await expect.poll(() => a.state.role).toBe("controller");
      pool.clear();
    });
  });
}

test("a theme change reaches the attached terminals and the host", async () => {
  const { pool, attaches, pane } = rig(referenceRenderer);
  const a = await pool.show("local:a", pane);
  await expect.poll(() => a.state.stream).toBe("live");
  const themes: string[] = [];
  const setTheme = a.view.setTheme.bind(a.view);
  a.view.setTheme = (theme) => {
    themes.push(theme.background);
    setTheme(theme);
  };
  (pool as unknown as { deps: PoolDeps }).deps.appearance = () => ({
    theme: { foreground: "#111111", background: "#fafafa", cursor: "#111111" },
    font: { family: "monospace", size: 13 },
    scheme: "light",
  });
  pool.restyle();
  await expect.poll(() => themes).toEqual(["#fafafa"]);
  expect(attaches[0].sent).toContainEqual({
    t: "facts",
    fg: "#111111",
    bg: "#fafafa",
    cursor: "#111111",
    color_scheme: "light",
  });
  pool.clear();
});

test("a change to Option as Meta or clipboard writes reaches the attached terminals", async () => {
  const { pool, pane } = rig(referenceRenderer);
  const a = await pool.show("local:a", pane);
  await expect.poll(() => a.state.stream).toBe("live");
  const applied: unknown[] = [];
  a.view.setInputOptions = (options) => applied.push(options);
  (pool as unknown as { deps: PoolDeps }).deps.appearance = () => ({
    theme: { foreground: "#111111", background: "#fafafa", cursor: "#111111" },
    font: { family: "monospace", size: 13 },
    scheme: "light",
    optionAsMeta: "left",
    clipboardWrite: false,
  });
  pool.restyle();
  await expect.poll(() => applied).toEqual([{ optionAsMeta: "left", clipboardWrite: false }]);
  pool.clear();
});
