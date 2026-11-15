import { describe, expect, test } from "vitest";
import { columnsOf, findMatches, Finder, type FindTarget } from "./find";

test("wide characters and emoji take two columns", () => {
  expect(columnsOf("a界b")).toEqual([0, 1, 3, 4]);
  // a surrogate pair is one character in one cell; a flag is two in two
  expect(columnsOf("👍x")).toEqual([0, 0, 2, 3]);
  expect(columnsOf("é")).toEqual([0, 1]);
});

describe("findMatches", () => {
  const lines = ["Error: one", "error two", "", "界 Error"];

  test("matches with or without case", () => {
    expect(findMatches(lines, "Error", { caseSensitive: true })).toEqual([
      { line: 0, start: 0, end: 5 },
      { line: 3, start: 3, end: 8 },
    ]);
    expect(findMatches(lines, "error", {})).toHaveLength(3);
  });

  test("matches a regular expression, and takes its text literally otherwise", () => {
    expect(findMatches(lines, "\\b[ot]\\w+", { regex: true, caseSensitive: true })).toEqual([
      { line: 0, start: 7, end: 10 },
      { line: 1, start: 6, end: 9 },
    ]);
    expect(findMatches(["a.c abc"], "a.c", { caseSensitive: true })).toEqual([{ line: 0, start: 0, end: 3 }]);
  });

  test("finds nothing for an empty query, an empty match or a broken expression", () => {
    expect(findMatches(lines, "", {})).toEqual([]);
    expect(findMatches(lines, "x*", { regex: true })).toEqual([]);
    expect(findMatches(lines, "(", { regex: true })).toEqual([]);
  });
});

describe("Finder", () => {
  function rig(lines: string[]) {
    const selected: string[] = [];
    const target = {
      screenLines: () => lines,
      setSelection: (a: { x: number; y: number }, b: { x: number; y: number }) => selected.push(`${a.y}:${a.x}-${b.x}`),
      scrollToScreenRow: () => {},
      scrollbar: () => ({ total: lines.length, offset: 1, length: 2 }),
      clearSearch: () => {},
      search: () => 0,
      searchMatchesInView: () => [],
    } as unknown as FindTarget;
    return { finder: new Finder(target), selected };
  }

  test("steps from the newest match up, and back down, wrapping around", () => {
    const { finder, selected } = rig(["Aa", "a", "A"]);
    const options = { caseSensitive: true };
    expect(finder.find("A", options)).toEqual({ found: true, count: 2 });
    finder.find("A", options);
    finder.find("A", options);
    finder.find("A", { ...options, backwards: true });
    expect(selected).toEqual(["2:0-0", "0:0-0", "2:0-0", "0:0-0"]);
  });

  test("highlights the matches in view, in viewport rows", () => {
    const { finder } = rig(["a", "xa", "a", "a"]);
    finder.find("a", { caseSensitive: true });
    expect(finder.matchesInView()).toEqual([
      { row: 0, start: 1, end: 2 },
      { row: 1, start: 0, end: 1 },
    ]);
  });
});
