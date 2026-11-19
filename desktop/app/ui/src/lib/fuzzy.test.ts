import { describe, expect, test } from "vitest";
import { fuzzyMatch } from "./fuzzy";

const score = (query: string, text: string) => fuzzyMatch(query, text)?.score ?? -Infinity;

describe("fuzzyMatch", () => {
  test("an empty query matches everything, with nothing marked", () => {
    expect(fuzzyMatch("", "anything")).toEqual({ score: 0, indices: [] });
  });

  test("the query as one piece marks its letters in place, ignoring case", () => {
    expect(fuzzyMatch("LOG", "Fix the login")?.indices).toEqual([8, 9, 10]);
  });

  test("a piece at the start of a word is preferred to one inside a word", () => {
    // "in" is inside "login" first, then starts the word "inbox"
    expect(fuzzyMatch("in", "login inbox")?.indices).toEqual([6, 7]);
  });

  test("the letters in order match when they are apart, at the starts of words first", () => {
    expect(fuzzyMatch("wi", "work items")?.indices).toEqual([0, 5]);
    expect(fuzzyMatch("cr", "code-review")?.indices).toEqual([0, 5]);
  });

  test("letters out of order, or missing, do not match", () => {
    expect(fuzzyMatch("ba", "abc")).toBeNull();
    expect(fuzzyMatch("xyz", "work items")).toBeNull();
  });

  test("a whole piece scores above scattered letters, and the text's start above a later word", () => {
    expect(score("set", "settings")).toBeGreaterThan(score("set", "show every tab"));
    expect(score("set", "settings")).toBeGreaterThan(score("set", "open settings"));
    expect(score("set", "open settings")).toBeGreaterThan(score("set", "reset"));
  });

  test("letters next to each other score above letters far apart", () => {
    expect(score("ab", "a-b-c")).toBeGreaterThan(score("ab", "a----------b"));
  });
});
