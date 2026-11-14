import { describe, expect, test } from "vitest";
import { hyperlinkAt, trimUrl, urlAt } from "./links";
import type { GhosttyCell, GhosttyRow } from "./types";

const black = { r: 0, g: 0, b: 0 };
const cell = (text: string, wide = 0): GhosttyCell => ({
  text,
  wide,
  foreground: black,
  background: black,
  bold: false,
  italic: false,
  invisible: false,
  strikethrough: false,
  overline: false,
  underline: 0,
  underlineColor: null,
  selected: false,
});

/** Rows of `width` cells; a row ending in "\" wraps into the next. */
function screen(width: number, ...lines: string[]): GhosttyRow[] {
  return lines.map((line) => {
    const wraps = line.endsWith("\\");
    const chars = [...(wraps ? line.slice(0, -1) : line)];
    const cells: GhosttyCell[] = [];
    for (const c of chars) {
      if (c === "界") cells.push(cell(c), cell("", 2));
      else cells.push(cell(c));
    }
    while (cells.length < width) cells.push(cell(""));
    return { cells, text: chars.join(""), isWrapContinuation: false, wrapsToNext: wraps };
  });
}

describe("trimUrl", () => {
  test("drops the punctuation that ends a sentence", () => {
    expect(trimUrl("https://pinrail.dev.")).toBe("https://pinrail.dev");
    expect(trimUrl("https://pinrail.dev/a?b=1),")).toBe("https://pinrail.dev/a?b=1");
    expect(trimUrl('https://x.io/"')).toBe("https://x.io/");
  });

  test("keeps a closing bracket the URL opened", () => {
    expect(trimUrl("https://en.wikipedia.org/wiki/Rust_(language)")).toBe(
      "https://en.wikipedia.org/wiki/Rust_(language)",
    );
    expect(trimUrl("https://en.wikipedia.org/wiki/Rust_(language))")).toBe(
      "https://en.wikipedia.org/wiki/Rust_(language)",
    );
  });
});

describe("urlAt", () => {
  test("finds the URL under a cell, and its cells", () => {
    const rows = screen(40, "see https://pinrail.dev/docs. ok");
    expect(urlAt(rows, 10, 0)).toEqual({ uri: "https://pinrail.dev/docs", spans: [{ row: 0, start: 4, end: 28 }] });
    // the full stop after it is not part of it
    expect(urlAt(rows, 28, 0)).toBeNull();
    expect(urlAt(rows, 1, 0)).toBeNull();
  });

  test("finds http, https and file URLs only", () => {
    expect(urlAt(screen(30, "http://a.io"), 0, 0)?.uri).toBe("http://a.io");
    expect(urlAt(screen(30, "file:///tmp/a.txt"), 3, 0)?.uri).toBe("file:///tmp/a.txt");
    expect(urlAt(screen(30, "ftp://a.io"), 0, 0)).toBeNull();
    expect(urlAt(screen(30, "mailto:a@b.io"), 0, 0)).toBeNull();
  });

  test("joins a URL that wraps onto the next row", () => {
    // a row wraps only when it is full
    const rows = screen(10, "go https:/\\", "/a.io/b c");
    const fromFirst = urlAt(rows, 5, 0);
    const fromSecond = urlAt(rows, 2, 1);
    expect(fromFirst).toEqual({
      uri: "https://a.io/b",
      spans: [
        { row: 0, start: 3, end: 10 },
        { row: 1, start: 0, end: 7 },
      ],
    });
    expect(fromSecond).toEqual(fromFirst);
  });

  test("does not join rows that only follow each other", () => {
    const rows = screen(10, "go https:", "//a.io/b c");
    expect(urlAt(rows, 5, 0)).toBeNull();
  });

  test("counts wide characters as two cells", () => {
    const rows = screen(30, "界 https://a.io 界");
    expect(urlAt(rows, 5, 0)?.spans).toEqual([{ row: 0, start: 3, end: 15 }]);
  });
});

describe("hyperlinkAt", () => {
  test("covers the cells of an OSC 8 link, across a wrap", () => {
    const rows = screen(6, "ab cd\\", "ef gh");
    // cells 3 to 5 of row 0 and 0 to 1 of row 1 link to the same URI
    const uriAt = (x: number, y: number) => ((y === 0 && x >= 3) || (y === 1 && x < 2) ? "https://a.io" : null);
    expect(hyperlinkAt(rows, uriAt, 0, 1)).toEqual({
      uri: "https://a.io",
      spans: [
        { row: 0, start: 3, end: 6 },
        { row: 1, start: 0, end: 2 },
      ],
    });
    expect(hyperlinkAt(rows, uriAt, 1, 0)).toBeNull();
  });
});
