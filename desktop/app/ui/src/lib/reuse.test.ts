import { describe, expect, test } from "vitest";
import { reuse, sameValue } from "./reuse";

describe("sameValue", () => {
  test("compares parsed JSON by what it holds", () => {
    expect(sameValue({ a: [1, { b: "x" }], c: null }, { c: null, a: [1, { b: "x" }] })).toBe(true);
    expect(sameValue({ a: [1, 2] }, { a: [1, 2, 3] })).toBe(false);
    expect(sameValue({ a: 1 }, { a: 1, b: undefined })).toBe(false);
    expect(sameValue({ a: null }, { a: {} })).toBe(false);
    expect(sameValue([], {})).toBe(false);
  });
});

describe("reuse", () => {
  const id = (v: { id: number }) => v.id;

  test("keeps the list when nothing changed", () => {
    const prev = [
      { id: 1, text: "a" },
      { id: 2, text: "b" },
    ];
    const next = JSON.parse(JSON.stringify(prev));
    expect(reuse(prev, next, id)).toBe(prev);
  });

  test("keeps the unchanged objects of a list that changed", () => {
    const prev = [
      { id: 1, text: "a" },
      { id: 2, text: "b" },
    ];
    const next = [
      { id: 1, text: "a" },
      { id: 2, text: "B" },
      { id: 3, text: "c" },
    ];
    const out = reuse(prev, next, id);
    expect(out).not.toBe(prev);
    expect(out[0]).toBe(prev[0]);
    expect(out[1]).toBe(next[1]);
    expect(out[2]).toBe(next[2]);
  });

  test("a new order is a change", () => {
    const prev = [{ id: 1 }, { id: 2 }];
    const out = reuse(prev, [{ id: 2 }, { id: 1 }], id);
    expect(out).not.toBe(prev);
    expect(out).toEqual([prev[1], prev[0]]);
  });

  test("a removed object is a change", () => {
    const prev = [{ id: 1 }, { id: 2 }];
    expect(reuse(prev, [{ id: 1 }], id)).toEqual([prev[0]]);
  });
});
