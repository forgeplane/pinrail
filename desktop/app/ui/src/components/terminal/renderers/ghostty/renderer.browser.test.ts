// libghostty-vt's screen drawn on a canvas, checked by its pixels.
import { expect, test } from "vitest";
import { GhosttyCore } from "./core";
import { CanvasRenderer } from "./renderer";
import { GhosttyRuntime } from "./runtime";

const background = { r: 0, g: 0, b: 0 };
const theme = { foreground: { r: 255, g: 255, b: 255 }, background, cursor: { r: 255, g: 255, b: 255 } };
const font = { family: "Menlo, monospace", size: 16, lineHeight: 1.25 };

async function draw(output: string) {
  const host = document.createElement("div");
  host.style.position = "relative";
  document.body.append(host);
  const renderer = new CanvasRenderer(host, font, { r: 60, g: 60, b: 120 });
  const { width, height } = renderer.cell;
  const core = GhosttyCore.create(await GhosttyRuntime.instantiate(), 20, 4, width, height, theme, () => {});
  core.write(new TextEncoder().encode(output));
  const snapshot = core.snapshot();
  renderer.draw(snapshot, true);
  const ctx = renderer.canvas.getContext("2d")!;
  const ratio = renderer.canvas.width / (20 * width);
  /** The pixels of cells `[col, col + span)` on `row`, as [r, g, b] each. */
  const pixels = (col: number, row: number, span = 1) => {
    const data = ctx.getImageData(
      Math.round(col * width * ratio),
      Math.round(row * height * ratio),
      Math.round(span * width * ratio),
      Math.round(height * ratio),
    );
    const out: { x: number; y: number; rgb: number[] }[] = [];
    for (let i = 0; i < data.data.length; i += 4) {
      const n = i / 4;
      out.push({ x: n % data.width, y: Math.floor(n / data.width), rgb: [...data.data.slice(i, i + 3)] });
    }
    return { all: out, width: data.width, height: data.height };
  };
  return { core, snapshot, pixels, dispose: () => (core.dispose(), renderer.dispose(), host.remove()) };
}

const lit = ({ rgb }: { rgb: number[] }) => rgb.some((c) => c > 60);

test("text is drawn in its colour in its own cell", async () => {
  const { pixels, dispose } = await draw("\x1b[31mX\x1b[0m");
  // red, though its edges blend into the black
  const red = pixels(0, 0).all.filter(({ rgb: [r, g, b] }) => r > 90 && r > g + 60 && r > b + 60);
  expect(red.length).toBeGreaterThan(5);
  expect(pixels(1, 0).all.filter(lit)).toHaveLength(0);
  dispose();
});

test("a box line runs unbroken across its cells", async () => {
  const { pixels, dispose } = await draw("──");
  const { all, width, height } = pixels(0, 0, 2);
  // some scanline is lit from the first pixel of the two cells to the last
  const rows = new Map<number, number>();
  for (const p of all) if (lit(p)) rows.set(p.y, (rows.get(p.y) ?? 0) + 1);
  expect([...rows.values()].some((n) => n === width)).toBe(true);
  // and it is the middle of the cell
  const middle = [...rows.entries()].find(([, n]) => n === width)![0];
  expect(Math.abs(middle - height / 2)).toBeLessThanOrEqual(2);
  dispose();
});

test("a wide emoji covers both its cells, drawn whole", async () => {
  const { snapshot, pixels, dispose } = await draw("👍z");
  expect(snapshot.rowData[0].cells[0].text).toBe("👍");
  expect(pixels(0, 0).all.filter(lit).length).toBeGreaterThan(5);
  expect(pixels(1, 0).all.filter(lit).length).toBeGreaterThan(5);
  dispose();
});

test("a curly underline is drawn in its own colour", async () => {
  const { snapshot, pixels, dispose } = await draw("\x1b[4:3m\x1b[58:2::0:255:0mu\x1b[0m");
  const cell = snapshot.rowData[0].cells[0];
  expect(cell.underline).toBe(3);
  expect(cell.underlineColor).toEqual({ r: 0, g: 255, b: 0 });
  const green = pixels(0, 0).all.filter(({ rgb: [r, g, b] }) => g > 150 && r < 100 && b < 100);
  expect(green.length).toBeGreaterThan(3);
  dispose();
});
