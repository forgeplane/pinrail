// Find and the screen reader mirror, with the real library in WebKit.
import { afterEach, expect, test } from "vitest";
import { GhosttyCore } from "./core";
import { Finder } from "./find";
import { Mirror, MirrorElements } from "./mirror";
import { GhosttyRuntime } from "./runtime";

const theme = { foreground: { r: 1, g: 1, b: 1 }, background: { r: 0, g: 0, b: 0 }, cursor: { r: 1, g: 1, b: 1 } };
let gone: (() => void)[] = [];
afterEach(() => {
  gone.forEach((g) => g());
  gone = [];
});

async function terminal(lines: string[]) {
  const core = GhosttyCore.create(await GhosttyRuntime.instantiate(), 40, 10, 8, 16, theme, () => {});
  gone.push(() => core.dispose());
  core.write(new TextEncoder().encode(lines.join("\r\n")));
  return core;
}

const lines = Array.from({ length: 60 }, (_, i) => (i % 20 === 5 ? `Marker ${i}` : `line ${i}`));

test("the screen's lines are its rows, scrollback first", async () => {
  const core = await terminal(lines);
  const text = core.screenLines();
  expect(text.slice(0, 2)).toEqual(["line 0", "line 1"]);
  expect(text.indexOf("Marker 45")).toBe(45);
});

test("plain find searches the scrollback, selects the match and highlights those in view", async () => {
  const core = await terminal(lines);
  const finder = new Finder(core);
  expect(finder.find("marker", {})).toEqual({ found: true, count: 3 });
  // the newest first, then older ones, scrolled into view
  expect(core.selectionText()).toBe("Marker");
  expect(core.snapshot().rowData.some((r) => r.text.startsWith("Marker 45"))).toBe(true);
  finder.find("marker", {});
  expect(core.snapshot().rowData.some((r) => r.text.startsWith("Marker 25"))).toBe(true);
  const inView = finder.matchesInView();
  const rows = core.snapshot().rowData;
  expect(inView.length).toBeGreaterThan(0);
  for (const span of inView) expect(rows[span.row].text.slice(span.start, span.end)).toBe("Marker");
  expect(finder.find("nowhere", {})).toEqual({ found: false, count: 0 });
  finder.clear();
  expect(finder.matchesInView()).toEqual([]);
});

test("case-sensitive and regular-expression find select the match", async () => {
  const core = await terminal([...lines, "界 Wide marker"]);
  const finder = new Finder(core);
  expect(finder.find("marker", { caseSensitive: true })).toEqual({ found: true, count: 1 });
  // after a wide character, in the right columns
  expect(core.selectionText()).toBe("marker");
  expect(finder.find("Marker \\d*5", { regex: true, caseSensitive: true })).toEqual({ found: true, count: 3 });
  expect(core.selectionText()).toBe("Marker 45");
  finder.find("Marker \\d*5", { regex: true, caseSensitive: true });
  expect(core.selectionText()).toBe("Marker 25");
  expect(core.snapshot().rowData.some((r) => r.text.startsWith("Marker 25"))).toBe(true);
});

test("the mirror's rows and live region are in the accessibility tree", async () => {
  const host = document.createElement("div");
  host.setAttribute("role", "document");
  host.setAttribute("aria-label", "claude in pinrail");
  document.body.append(host);
  const elements = new MirrorElements(host);
  const mirror = new Mirror(elements);
  gone.push(() => (mirror.dispose(), elements.dispose(), host.remove()));
  const core = await terminal(["$ make", "Building"]);
  mirror.update(core.snapshot().rowData);
  const items = host.querySelectorAll('[role="listitem"]');
  expect(items).toHaveLength(10);
  expect([items[0].textContent, items[1].textContent]).toEqual(["$ make", "Building"]);
  mirror.output("\x1b[32mDone\x1b[0m\r\n");
  await new Promise((r) => setTimeout(r, 300));
  expect(host.querySelector('[aria-live="polite"]')?.textContent).toBe("Done");
  // off screen, but not hidden from assistive technology
  const box = elements.element.getBoundingClientRect();
  expect(box.width).toBeLessThanOrEqual(1);
  expect(getComputedStyle(elements.element).display).not.toBe("none");
});
