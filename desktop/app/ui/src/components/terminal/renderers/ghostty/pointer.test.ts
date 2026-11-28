import { describe, expect, test } from "vitest";
import { autoscrollRows, buttonOf, modsOf, pressOf, wheelRows } from "./pointer";

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

test("presses count as clicks where the browser leaves the count out, as Chromium does", () => {
  const at = (clientX: number, timeStamp: number, detail = 0) => ({ clientX, clientY: 10, timeStamp, detail });
  const first = pressOf(null, at(20, 1000));
  const second = pressOf(first, at(22, 1200));
  const third = pressOf(second, at(21, 1400));
  expect([first.count, second.count, third.count]).toEqual([1, 2, 3]);
  // too late, or too far, starts again
  expect(pressOf(third, at(21, 2000)).count).toBe(1);
  expect(pressOf(first, at(40, 1100)).count).toBe(1);
  // WebKit's own count, from the system's settings, wins
  expect(pressOf(null, at(20, 1000, 2)).count).toBe(2);
});
