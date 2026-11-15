// The libghostty view restores the host's binary snapshot exactly.
import { expect, test } from "vitest";
import snapshotUrl from "../../testing/ghostty-snapshot.bin?url";
import host from "../../testing/ghostty-snapshot.json";
import { GhosttyCore } from "./core";
import { GhosttyRuntime } from "./runtime";
import type { BenchHooks } from "../../view";
import { BINARY_FORMAT, ghosttyRenderer } from "./view";

test("a snapshot the host wrote shows the host's screen, history and title", async () => {
  expect(host.format).toBe(BINARY_FORMAT);
  const bytes = new Uint8Array(await (await fetch(snapshotUrl)).arrayBuffer());
  const theme = { foreground: { r: 1, g: 1, b: 1 }, background: { r: 0, g: 0, b: 0 }, cursor: { r: 1, g: 1, b: 1 } };
  const core = GhosttyCore.create(await GhosttyRuntime.instantiate(), 80, 24, 8, 16, theme, () => {});
  core.restoreSnapshot(bytes, 80, 24, 8, 16);
  const { total, length } = core.scrollbar();
  const retained = total - length;
  const lines = core.screenLines();
  expect(retained).toBe(host.retainedLines);
  expect(lines.slice(0, retained).join("\n")).toBe(host.history);
  expect(lines.slice(retained).join("\n").trimEnd()).toBe(host.screen.trimEnd());
  expect(core.title()).toBe(host.title);
  core.dispose();
});

test("output is drawn in the frame that parses it", async () => {
  const host = document.createElement("div");
  host.style.cssText = "width: 400px; height: 200px; position: relative";
  document.body.append(host);
  const view = ghosttyRenderer.create();
  await view.mount(host, {
    cols: 40,
    rows: 10,
    scrollback: 100,
    theme: { foreground: "#e6e6e6", background: "#101010", cursor: "#e6e6e6" },
    font: { family: "monospace", size: 13 },
  });
  // let the first draw after mounting pass
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  let frame = 0;
  let counting = true;
  const tick = () => {
    frame++;
    if (counting) requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  const hooks = view as unknown as BenchHooks;
  for (const echo of ["a", "b", "c"]) {
    let parsedIn = -1;
    const drawnIn = new Promise<number>((resolve) => {
      const off = hooks.onFrameDrawn(() => {
        off();
        resolve(frame);
      });
    });
    view.write(new TextEncoder().encode(echo), () => (parsedIn = frame));
    expect(await drawnIn).toBe(parsedIn);
  }
  counting = false;
  view.dispose();
  host.remove();
});

test("the echo of a key is drawn before the write returns", async () => {
  const host = document.createElement("div");
  host.style.cssText = "width: 400px; height: 200px; position: relative";
  document.body.append(host);
  const view = ghosttyRenderer.create();
  await view.mount(host, {
    cols: 40,
    rows: 10,
    scrollback: 100,
    theme: { foreground: "#e6e6e6", background: "#101010", cursor: "#e6e6e6" },
    font: { family: "monospace", size: 13 },
  });
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  const hooks = view as unknown as BenchHooks;
  let drawn = 0;
  hooks.onFrameDrawn(() => drawn++);
  let consumed = false;
  // output with no key before it waits for the next frame
  view.write(new TextEncoder().encode("$ "), () => (consumed = true));
  expect([consumed, drawn]).toEqual([false, 0]);
  await new Promise((r) => requestAnimationFrame(r));
  expect(consumed).toBe(true);
  drawn = 0;
  view.onInput((bytes) => view.write(bytes, () => {}));
  hooks
    .inputElement()!
    .dispatchEvent(new KeyboardEvent("keydown", { key: "a", code: "KeyA", bubbles: true, cancelable: true }));
  expect(drawn).toBe(1);
  view.dispose();
  host.remove();
});

test("a program's copy reaches the clipboard only when the view allows it", async () => {
  const written: string[] = [];
  const clipboard = navigator.clipboard;
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: async (text: string) => void written.push(text) },
  });
  try {
    for (const clipboardWrite of [true, false]) {
      const host = document.createElement("div");
      document.body.append(host);
      const view = ghosttyRenderer.create();
      await view.mount(host, {
        cols: 40,
        rows: 10,
        scrollback: 100,
        theme: { foreground: "#e6e6e6", background: "#101010", cursor: "#e6e6e6" },
        font: { family: "monospace", size: 13 },
        clipboardWrite,
      });
      // "allowed" and "refused", base64
      const text = clipboardWrite ? "YWxsb3dlZA==" : "cmVmdXNlZA==";
      await new Promise<void>((resolve) => view.write(new TextEncoder().encode(`\x1b]52;c;${text}\x07`), resolve));
      view.dispose();
      host.remove();
    }
  } finally {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: clipboard });
  }
  expect(written).toEqual(["allowed"]);
});
