import { describe, expect, test } from "vitest";
import { scaleRect, zoomCorrection } from "./viewport";

describe("an element's place in the window under the interface's zoom", () => {
  test("an engine that reports before the zoom is scaled up by it", () => {
    // #root is 1000px wide before the zoom, and reported so
    expect(zoomCorrection(1.08, 1000, 1000)).toBeCloseTo(1.08);
  });

  test("an engine that reports in the window's pixels is left as it is", () => {
    expect(zoomCorrection(1.08, 1080, 1000)).toBe(1);
    expect(zoomCorrection(0.92, 920, 1000)).toBe(1);
  });

  test("without a zoom, nothing changes", () => {
    expect(zoomCorrection(1, 1000, 1000)).toBe(1);
    expect(zoomCorrection(NaN, 1000, 1000)).toBe(1);
  });

  test("a rectangle scales from the window's corner", () => {
    expect(scaleRect({ left: 100, top: 50, width: 20, height: 10 }, 1.5)).toEqual({
      left: 150,
      top: 75,
      width: 30,
      height: 15,
      right: 180,
      bottom: 90,
    });
  });
});
