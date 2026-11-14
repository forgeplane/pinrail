import { describe, expect, test } from "vitest";
import { autoscrollRows, buttonOf, modsOf, wheelRows } from "./pointer";

describe("the wheel", () => {
  test("scrolls whole rows and keeps the pixels left over", () => {
    let step = wheelRows(0, { deltaY: 30, deltaMode: 0 }, 20, 24);
    expect(step).toEqual({ rows: 1, pending: 10 });
    step = wheelRows(step.pending, { deltaY: 15, deltaMode: 0 }, 20, 24);
    expect(step).toEqual({ rows: 1, pending: 5 });
    expect(wheelRows(0, { deltaY: -45, deltaMode: 0 }, 20, 24)).toEqual({ rows: -2, pending: -5 });
  });

  test("counts lines and pages in rows", () => {
    expect(wheelRows(0, { deltaY: 3, deltaMode: 1 }, 20, 24).rows).toBe(3);
    expect(wheelRows(0, { deltaY: -1, deltaMode: 2 }, 20, 24).rows).toBe(-24);
  });
});

test("a drag scrolls faster the further it is past the edge", () => {
  expect(autoscrollRows(1, 20)).toBe(1);
  expect(autoscrollRows(45, 20)).toBe(2);
  expect(autoscrollRows(10_000, 20)).toBe(10);
});

test("mouse buttons and modifiers map to libghostty's", () => {
  expect([buttonOf(0), buttonOf(1), buttonOf(2), buttonOf(3)]).toEqual([1, 3, 2, null]);
  expect(modsOf({ shiftKey: true, ctrlKey: false, altKey: true, metaKey: false } as MouseEvent)).toBe(5);
});
