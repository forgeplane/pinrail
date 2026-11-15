import { describe, expect, test } from "vitest";
import { cellKind, splitRuns } from "./renderer";
import type { GhosttyCell } from "./types";

const white = { r: 255, g: 255, b: 255 };
const red = { r: 255, g: 0, b: 0 };
const cell = (text: string, over: Partial<GhosttyCell> = {}): GhosttyCell => ({
  text,
  wide: 0,
  foreground: white,
  background: { r: 0, g: 0, b: 0 },
  bold: false,
  italic: false,
  invisible: false,
  strikethrough: false,
  overline: false,
  underline: 0,
  underlineColor: null,
  selected: false,
  ...over,
});
const row = (...cells: GhosttyCell[]) => splitRuns(cells).map((r) => `${r.kind} ${r.start}-${r.end}`);

describe("the runs of a row", () => {
  test("join text cells of one style, and blanks", () => {
    expect(row(cell("l"), cell("s"), cell(" "), cell(""), cell("-"), cell("l"))).toEqual([
      "text 0-2",
      "blank 2-4",
      "text 4-6",
    ]);
  });

  test("break where the colour, the weight or the slant changes", () => {
    expect(
      row(cell("a"), cell("b", { foreground: red }), cell("c", { bold: true }), cell("d", { italic: true })),
    ).toEqual(["text 0-1", "text 1-2", "text 2-3", "text 3-4"]);
  });

  test("keep each wide, drawn and cached glyph alone", () => {
    // a wide cluster, then the spacer after it, a box line and an emoji
    expect(row(cell("你"), cell("", { wide: 2 }), cell("─"), cell("─"), cell("👍🏽"))).toEqual([
      "glyph 0-1",
      "blank 1-2",
      "drawn 2-3",
      "drawn 3-4",
      "cached 4-5",
    ]);
  });

  test("cache only colour emoji, which the font draws slowly", () => {
    expect(["你", "é̃", "λ", "❤️", "🇬🇷", "👩‍💻"].map((t) => cellKind(cell(t)))).toEqual([
      "glyph",
      "glyph",
      "text",
      "cached",
      "cached",
      "cached",
    ]);
  });

  test("draw no text for an invisible cell", () => {
    expect(cellKind(cell("x", { invisible: true }))).toBe("blank");
  });
});
