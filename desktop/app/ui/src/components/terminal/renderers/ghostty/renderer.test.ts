import { describe, expect, test } from "vitest";
import { cellKind, movedBand, splitRuns } from "./renderer";
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

  test("draw a private-use icon from the symbols font, and the Powerline separators from the cell's edges", () => {
    // a Nerd Font house, a Material icon past the BMP, and a Powerline arrow
    expect(["\uf015", "\u{f0a1e}", "\ue0b0"].map((t) => cellKind(cell(t)))).toEqual(["icon", "icon", "drawn"]);
    expect(row(cell("\uf015"), cell("\uf015"))).toEqual(["icon 0-1", "icon 1-2"]);
  });

  test("draw no text for an invisible cell", () => {
    expect(cellKind(cell("x", { invisible: true }))).toBe("blank");
  });
});

describe("the band of rows that moved", () => {
  /** The band from what the canvas shows to what it should show, every
   *  row changed, as after a scroll. */
  const band = (shown: string[], now: string[]) => movedBand(shown, new Map(now.entries()), shown.length);

  test("is found when output scrolls up a row", () => {
    expect(band(["a", "b", "c", "d"], ["b", "c", "d", "e"])).toEqual({ from: 0, to: 3, by: 1 });
  });

  test("is found when the wheel scrolls back a row", () => {
    expect(band(["b", "c", "d", "e"], ["a", "b", "c", "d"])).toEqual({ from: 1, to: 4, by: -1 });
  });

  test("is found for a move of several rows either way", () => {
    expect(band(["a", "b", "c", "d", "e", "f"], ["d", "e", "f", "x", "y", "z"])).toEqual({ from: 0, to: 3, by: 3 });
    expect(band(["d", "e", "f", "x", "y", "z"], ["a", "b", "c", "d", "e", "f"])).toEqual({ from: 3, to: 6, by: -3 });
  });

  test("keeps the longest band, past rows that many rows show", () => {
    const blank = "";
    expect(band(["a", blank, blank, "b", "c", "d"], [blank, blank, "b", "c", "d", "e"])).toEqual({
      from: 0,
      to: 5,
      by: 1,
    });
  });

  test("is none when no rows moved together", () => {
    expect(band(["a", "b", "c"], ["x", "y", "z"])).toBeNull();
    expect(band(["a", "b", "c"], ["b", "x", "y"])).toBeNull();
  });
});
