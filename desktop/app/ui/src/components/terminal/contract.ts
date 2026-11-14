// The contract every terminal renderer's adapter passes: it parses a
// session's output and says when, restores each snapshot format it lists,
// encodes keys and paste in the program's modes, keeps its emulator's
// replies apart from typed input, selects and copies, searches, resizes,
// and leaves nothing behind when disposed. It runs in a real browser, with
// vitest.browser.config.ts, against every registered renderer.

import { afterEach, beforeEach, describe, expect, test } from "vitest";
import { userEvent } from "vitest/browser";
import type { InputKind, TerminalRenderer, TerminalView, ViewOptions } from "./view";

const encoder = new TextEncoder();
const bytes = (text: string) => encoder.encode(text);
const text = (data: Uint8Array) => new TextDecoder().decode(data);

/** The text a restored snapshot shows, which `snapshots` must hold. */
export const RESTORED = "restored-screen";

export interface ContractFixtures {
  /** A screen in each binary format a renderer may list, showing
   *  `RESTORED`. The `vt` format needs none: the contract writes one. */
  snapshots?: Readonly<Record<string, Uint8Array>>;
}

/** Output like an agent's: a title, colours, cursor moves, an erased line,
 *  wide characters, and a marker on the last line. */
export const SESSION =
  "\x1b]2;Agent at work\x07" +
  "\x1b[1;32m✻\x1b[0m Thinking…\r\n" +
  Array.from({ length: 60 }, (_, i) => `\x1b[3${i % 8}mline ${i} 漢字 🚀\x1b[0m\r\n`).join("") +
  "\x1b[2A\x1b[2Kredrawn\r\n\x1b[Bsession-end-marker\r\n";

const OPTIONS: ViewOptions = {
  cols: 80,
  rows: 24,
  scrollback: 1000,
  theme: { foreground: "#e6e6e6", background: "#101010", cursor: "#e6e6e6" },
  font: { family: "monospace", size: 13 },
};

/** Writes `data` in chunks, and resolves once every chunk was consumed. */
function writeAll(view: TerminalView, data: Uint8Array, chunk = 4096): Promise<number> {
  return new Promise((resolve) => {
    let pending = Math.ceil(data.length / chunk);
    let consumed = 0;
    for (let at = 0; at < data.length; at += chunk) {
      view.write(data.subarray(at, at + chunk), () => {
        consumed++;
        if (--pending === 0) resolve(consumed);
      });
    }
  });
}

export function describeContract(name: string, make: () => Promise<TerminalRenderer>, fixtures: ContractFixtures = {}) {
  describe(`the ${name} renderer`, () => {
    let renderer: TerminalRenderer;
    let host: HTMLElement;
    let view: TerminalView;
    let inputs: { data: string; kind: InputKind }[];
    let replies: string[];

    beforeEach(async () => {
      renderer = await make();
      await renderer.load();
      host = document.createElement("div");
      host.style.cssText = "width: 800px; height: 480px; position: relative";
      document.body.append(host);
      view = renderer.create();
      await view.mount(host, OPTIONS);
      inputs = [];
      replies = [];
      view.onInput((data, kind) => inputs.push({ data: text(data), kind }));
      view.onReply((data) => replies.push(text(data)));
    });

    afterEach(() => {
      view.dispose();
      host.remove();
    });

    test("parses a session's output and says when each write is consumed", async () => {
      const data = bytes(SESSION);
      const consumed = await writeAll(view, data, 1000);
      expect(consumed).toBe(Math.ceil(data.length / 1000));
      expect(view.search("session-end-marker", {}).found).toBe(true);
    });

    test("restores each snapshot format it lists, replacing the screen", async () => {
      expect(view.snapshotFormats).toContain("vt");
      await writeAll(view, bytes("before-restore\r\n"));
      for (const format of view.snapshotFormats) {
        const snapshot = format === "vt" ? bytes(`\x1bc${RESTORED}\r\n`) : fixtures.snapshots?.[format];
        expect(snapshot, `a fixture for ${format}`).toBeDefined();
        await view.restore(format, snapshot!);
        expect(view.search(RESTORED, {}).found, format).toBe(true);
        expect(view.search("before-restore", {}).found, format).toBe(false);
        view.reset();
        expect(view.search(RESTORED, {}).found, `${format} after reset`).toBe(false);
      }
    });

    test("encodes keys and paste in the program's modes", async () => {
      view.focus();
      await userEvent.keyboard("a{Enter}{ArrowUp}");
      // application cursor keys and bracketed paste, as a program asks
      await writeAll(view, bytes("\x1b[?1h\x1b[?2004h"));
      await userEvent.keyboard("{ArrowUp}");
      const paste = new DataTransfer();
      paste.setData("text/plain", "pasted");
      document.activeElement?.dispatchEvent(
        new ClipboardEvent("paste", { clipboardData: paste, bubbles: true, cancelable: true }),
      );
      const keys = inputs.filter((i) => i.kind === "key").map((i) => i.data);
      expect(keys.join("")).toBe("a\r\x1b[A\x1bOA");
      expect(inputs.filter((i) => i.kind === "paste").map((i) => i.data)).toEqual(["\x1b[200~pasted\x1b[201~"]);
    });

    test("sends its emulator's replies as replies, never as input", async () => {
      await writeAll(view, bytes("\x1b[6n"));
      await expect.poll(() => replies.length).toBeGreaterThan(0);
      // a cursor position report: ESC [ row ; column R
      const reply = replies.join("");
      expect(reply.slice(0, 2)).toBe("\x1b[");
      expect(reply.slice(2)).toMatch(/^\d+;\d+R/);
      expect(inputs).toEqual([]);
    });

    test("selects a word, reports it, and gives it to copy", async () => {
      const selected: (string | null)[] = [];
      view.onSelectionChange((selection) => selected.push(selection));
      await writeAll(view, bytes("\x1b[H\x1b[2Jpickme rest\r\n"));
      const { cellPx } = view.measure();
      const box = host.getBoundingClientRect();
      await userEvent.dblClick(host, {
        position: { x: Math.round(cellPx[0] * 2.5), y: Math.round(cellPx[1] / 2) },
      });
      expect(box.width).toBeGreaterThan(0);
      await expect.poll(() => view.selection()).toBe("pickme");
      expect(selected).toContain("pickme");
    });

    test("finds text in the screen and the scrollback", async () => {
      await writeAll(view, bytes(Array.from({ length: 200 }, (_, i) => `row ${i}\r\n`).join("")));
      expect(view.search("row 3", {}).found).toBe(true);
      expect(view.search("row 199", {}).found).toBe(true);
      expect(view.search("not written anywhere", {}).found).toBe(false);
    });

    test("measures the grid that fits its element, and resizes", () => {
      const { cols, rows, cellPx } = view.measure();
      expect(cellPx[0]).toBeGreaterThan(0);
      expect(cellPx[1]).toBeGreaterThan(0);
      expect(cols).toBe(Math.floor(800 / cellPx[0]));
      expect(rows).toBe(Math.floor(480 / cellPx[1]));
      view.resize(40, 10);
      view.resize(cols, rows);
    });

    test("leaves nothing behind when disposed", async () => {
      const extra = document.createElement("div");
      extra.style.cssText = "width: 400px; height: 200px";
      document.body.append(extra);
      for (let i = 0; i < 10; i++) {
        const other = renderer.create();
        await other.mount(extra, OPTIONS);
        let heard = false;
        other.onInput(() => (heard = true));
        other.dispose();
        expect(extra.childElementCount, `after view ${i}`).toBe(0);
        extra.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true }));
        expect(heard).toBe(false);
      }
      extra.remove();
    });
  });
}
