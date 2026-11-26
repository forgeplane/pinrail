// The libghostty terminal's pointer, with the real library, drawn by the
// canvas renderer, and pointer events in WebKit.
import { afterEach, expect, test } from "vitest";
import { GhosttyCore } from "./core";
import type { LinkSpan } from "./links";
import { Pointer, type PointerOptions } from "./pointer";
import { CanvasRenderer } from "./renderer";
import { GhosttyRuntime } from "./runtime";

const theme = {
  foreground: { r: 230, g: 230, b: 230 },
  background: { r: 0, g: 0, b: 0 },
  cursor: { r: 1, g: 1, b: 1 },
};
const text = (bytes: Uint8Array) => new TextDecoder().decode(bytes);
let gone: (() => void)[] = [];
afterEach(() => {
  gone.forEach((g) => g());
  gone = [];
});

async function rig(options: Partial<PointerOptions> = {}) {
  const host = document.createElement("div");
  host.style.cssText = "position:absolute;left:0;top:0";
  document.body.append(host);
  const renderer = new CanvasRenderer(
    host,
    { family: "Menlo, monospace", size: 16, lineHeight: 1.2 },
    { r: 0, g: 0, b: 200 },
  );
  const { width, height } = renderer.cell;
  const core = GhosttyCore.create(await GhosttyRuntime.instantiate(), 40, 10, width, height, theme, () => {});
  let snapshot = core.snapshot();
  const draw = () => {
    renderer.invalidate();
    snapshot = core.snapshot();
    renderer.draw(snapshot, true);
  };
  draw();
  const sent: [string, string][] = [];
  const links: string[] = [];
  let redraws = 0;
  let scrolls = 0;
  let shown: readonly LinkSpan[] = [];
  const pointer = new Pointer(renderer.canvas, core, {
    mac: true,
    cell: () => renderer.cell,
    rows: () => snapshot.rowData,
    emit: (bytes, kind) => sent.push([kind, text(bytes)]),
    redraw: () => {
      redraws++;
      draw();
    },
    scrolled: () => {
      scrolls++;
      snapshot = core.snapshot();
      renderer.draw(snapshot, true);
    },
    showLink: (spans) => {
      shown = spans;
      renderer.setLink(spans);
      draw();
    },
    onLink: (uri) => links.push(uri),
    focus: () => {},
    ...options,
  });
  gone.push(() => (pointer.dispose(), renderer.dispose(), core.dispose(), host.remove()));
  const write = (s: string) => {
    core.write(new TextEncoder().encode(s));
    draw();
  };
  /** The middle of a cell, in client coordinates; a row past the grid is
   *  past its edge. */
  const at = (col: number, row: number) => {
    const rect = renderer.canvas.getBoundingClientRect();
    return { clientX: rect.left + (col + 0.5) * width, clientY: rect.top + (row + 0.5) * height };
  };
  const fire = (type: string, col: number, row: number, init: PointerEventInit = {}) =>
    renderer.canvas.dispatchEvent(
      new PointerEvent(type, { bubbles: true, cancelable: true, pointerId: 1, ...at(col, row), ...init }),
    );
  const drag = (from: [number, number], to: [number, number], init: PointerEventInit = {}) => {
    fire("pointerdown", ...from, { buttons: 1, detail: 1, ...init });
    fire("pointermove", ...to, { buttons: 1, ...init });
    fire("pointerup", ...to, { detail: 1, ...init });
  };
  const counts = () => ({ redraws, scrolls });
  return { core, renderer, sent, links, shown: () => shown, write, fire, drag, counts };
}

test("a drag selects cells, and the selection stays through new output", async () => {
  const { core, write, drag } = await rig();
  write("hello world");
  drag([0, 0], [4, 0]);
  expect(core.selectionText()).toBe("hello");
  // a click without a drag clears it
  drag([2, 0], [2, 0]);
  expect(core.selectionText()).toBeNull();
  drag([6, 0], [10, 0]);
  write("\r\n" + "more\r\n".repeat(30));
  expect(core.selectionText()).toBe("world");
});

test("a double-click selects a word and a triple-click a line", async () => {
  const { core, write, fire } = await rig();
  write("one two-three four");
  fire("pointerdown", 5, 0, { detail: 2 });
  // libghostty's word boundaries keep a hyphenated word whole
  expect(core.selectionText()).toBe("two-three");
  fire("pointerdown", 5, 0, { detail: 3 });
  expect(core.selectionText()).toBe("one two-three four");
});

test("Shift-click extends the selection, and Option-drag selects a rectangle", async () => {
  const { core, write, drag, fire } = await rig();
  write("abcdef\r\nghijkl");
  drag([1, 0], [2, 0]);
  fire("pointerdown", 3, 1, { shiftKey: true, buttons: 1, detail: 1 });
  fire("pointerup", 3, 1, { shiftKey: true });
  expect(core.selectionText()).toBe("bcdef\nghij");
  drag([1, 0], [2, 1], { altKey: true });
  expect(core.selectionText()).toBe("bc\nhi");
});

test("dragging past the top edge scrolls and selects into the scrollback", async () => {
  const { core, write, fire } = await rig();
  write(Array.from({ length: 40 }, (_, i) => `line ${i}`).join("\r\n"));
  const top = core.scrollbar().offset;
  // from the end of the last line
  fire("pointerdown", 6, 9, { buttons: 1, detail: 1 });
  fire("pointermove", 3, -3, { buttons: 1 });
  await new Promise((r) => setTimeout(r, 260));
  fire("pointerup", 3, -3);
  expect(core.scrollbar().offset).toBeLessThan(top);
  const selected = core.selectionText() ?? "";
  // from a row that was above the viewport down to the last line
  expect(selected.split("\n").length).toBeGreaterThan(10);
  expect(selected.endsWith("line 39")).toBe(true);
});

test("clicks go to a program that asks for the mouse, unless Shift is held", async () => {
  const { core, sent, write, drag } = await rig();
  write("hello\x1b[?1000h\x1b[?1006h");
  drag([2, 1], [2, 1]);
  expect(sent).toEqual([
    ["mouse", "\x1b[<0;3;2M"],
    ["mouse", "\x1b[<0;3;2m"],
  ]);
  drag([0, 0], [4, 0], { shiftKey: true });
  expect(sent).toHaveLength(2);
  expect(core.selectionText()).toBe("hello");
});

test("the wheel scrolls the scrollback, or sends arrows on the alternate screen", async () => {
  const { core, renderer, sent, write, counts } = await rig();
  write(Array.from({ length: 40 }, (_, i) => `line ${i}`).join("\r\n"));
  const wheel = (deltaY: number) =>
    renderer.canvas.dispatchEvent(new WheelEvent("wheel", { deltaY, deltaMode: 1, bubbles: true, cancelable: true }));
  const bottom = core.scrollbar().offset;
  wheel(-3);
  expect(core.scrollbar().offset).toBe(bottom - 3);
  expect(sent).toEqual([]);
  // a scroll draws the rows that moved, and leaves the selection alone
  expect(counts()).toEqual({ redraws: 0, scrolls: 1 });
  write("\x1b[?1049h");
  wheel(2);
  expect(sent).toEqual([
    ["key", "\x1b[B"],
    ["key", "\x1b[B"],
  ]);
});

test("a link is underlined under the pointer and opens with Cmd-click", async () => {
  const { sent, links, shown, write, fire } = await rig();
  write("docs: https://pinrail.dev/docs.\r\n\x1b]8;;https://example.com/a\x1b\\click here\x1b]8;;\x1b\\ plain");
  fire("pointermove", 10, 0);
  await new Promise((r) => setTimeout(r, 150));
  expect(shown()).toEqual([{ row: 0, start: 6, end: 30 }]);
  fire("pointermove", 3, 1);
  await new Promise((r) => setTimeout(r, 150));
  expect(shown()).toEqual([{ row: 1, start: 0, end: 10 }]);
  fire("pointermove", 13, 1);
  await new Promise((r) => setTimeout(r, 150));
  expect(shown()).toEqual([]);
  // a plain click on a link selects as usual; Cmd-click opens it
  fire("pointerdown", 3, 1, { buttons: 1, detail: 1 });
  fire("pointerup", 3, 1);
  fire("pointerdown", 3, 1, { buttons: 1, detail: 1, metaKey: true });
  fire("pointerdown", 10, 0, { buttons: 1, detail: 1, metaKey: true });
  expect(links).toEqual(["https://example.com/a", "https://pinrail.dev/docs"]);
  expect(sent).toEqual([]);
});
