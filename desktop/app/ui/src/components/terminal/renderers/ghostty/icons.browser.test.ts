// Nerd Font icons: private-use code points drawn from the bundled symbols
// font, which loads only for a screen that shows one. The tests share the
// page's fonts, so the screen without an icon comes first.
import { expect, test } from "vitest";
import { GhosttyCore } from "./core";
import { iconFontLoaded } from "./icons";
import { CanvasRenderer } from "./renderer";
import { GhosttyRuntime } from "./runtime";

const background = { r: 0, g: 0, b: 0 };
const theme = { foreground: { r: 255, g: 255, b: 255 }, background, cursor: { r: 255, g: 255, b: 255 } };
const font = { family: "Menlo, monospace", size: 16, lineHeight: 1.25 };

async function terminal() {
  const host = document.createElement("div");
  host.style.position = "relative";
  document.body.append(host);
  const renderer = new CanvasRenderer(host, font, { r: 60, g: 60, b: 120 });
  const { width, height } = renderer.cell;
  const core = GhosttyCore.create(await GhosttyRuntime.instantiate(), 20, 3, width, height, theme, () => {});
  const ctx = renderer.canvas.getContext("2d")!;
  // the canvas has its size once it has drawn
  const ratio = () => renderer.canvas.width / (20 * width);
  /** The lit pixels of cells `[col, col + span)` on row 0, by x and y in
   *  device pixels from the first cell's corner. */
  const lit = (col: number, span = 1) => {
    const data = ctx.getImageData(
      Math.round(col * width * ratio()),
      0,
      Math.round(span * width * ratio()),
      Math.round(height * ratio()),
    );
    const out: { x: number; y: number }[] = [];
    for (let i = 0; i < data.data.length; i += 4)
      if (data.data[i]! > 60 || data.data[i + 1]! > 60 || data.data[i + 2]! > 60)
        out.push({ x: (i / 4) % data.width, y: Math.floor(i / 4 / data.width) });
    return out;
  };
  const show = (output: string) => {
    core.write(new TextEncoder().encode(output));
    renderer.invalidate();
    renderer.draw(core.snapshot(), true);
  };
  return {
    show,
    lit,
    cell: () => ({ w: width * ratio(), h: height * ratio() }),
    dispose: () => (core.dispose(), renderer.dispose(), host.remove()),
  };
}

/** Whether the font file was fetched: not the module that names its
 *  address, which the test server serves as `?url`. */
const fetched = () =>
  performance
    .getEntriesByType("resource")
    .some((e) => /SymbolsNerdFont[^/?]*\.woff2/.test(e.name) && !/[?&]url\b/.test(e.name));

test("a screen without a private-use code point does not fetch the symbols font", async () => {
  const t = await terminal();
  t.show("$ ls -la ─ ▓ ⠋  👍🏽 日本\r\n");
  await new Promise((done) => setTimeout(done, 200));
  expect(fetched()).toBe(false);
  t.dispose();
});

test("a Nerd Font icon is drawn from the symbols font, sized to its room", async () => {
  const t = await terminal();
  // a house followed by a blank cell, which gives it two cells of room
  t.show(" home");
  await iconFontLoaded();
  expect(fetched()).toBe(true);
  t.show("");
  const cell = t.cell();
  const icon = t.lit(0, 2);
  expect(icon.length).toBeGreaterThan(20);
  const xs = icon.map((p) => p.x);
  const ys = icon.map((p) => p.y);
  const [width, height] = [Math.max(...xs) - Math.min(...xs) + 1, Math.max(...ys) - Math.min(...ys) + 1];
  // scaled as a whole until it reaches 72% of the line's height or 75%
  // of its two cells' width, and within its two cells
  const reach = Math.max(height / (0.72 * cell.h), width / (1.5 * cell.w));
  expect(reach).toBeGreaterThan(0.9);
  expect(reach).toBeLessThan(1.1);
  expect(width).toBeGreaterThan(cell.w);
  expect(Math.max(...xs)).toBeLessThan(cell.w * 2);
  // centred in the line
  const middle = (Math.min(...ys) + Math.max(...ys)) / 2;
  expect(Math.abs(middle - cell.h / 2)).toBeLessThan(cell.h * 0.1);
  t.dispose();
});
