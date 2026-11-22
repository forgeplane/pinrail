import { describe, expect, test } from "vitest";
import { contrast, legible, MIN_CONTRAST } from "./contrast";

const cream = { r: 253, g: 246, b: 227 };
const night = { r: 40, g: 42, b: 54 };

describe("the contrast floor of terminal text", () => {
  test("darkens a colour that nearly vanishes on a light background, just to the floor", () => {
    const white = { r: 238, g: 238, b: 238 };
    const shown = legible(white, cream);
    expect(contrast(shown, cream)).toBeGreaterThanOrEqual(MIN_CONTRAST);
    expect(contrast(shown, cream)).toBeLessThan(MIN_CONTRAST + 0.1);
    expect(shown.r).toBeLessThan(white.r);
  });

  test("lightens a colour that nearly vanishes on a dark background", () => {
    const navy = { r: 30, g: 30, b: 90 };
    const shown = legible(navy, night);
    expect(contrast(shown, night)).toBeGreaterThanOrEqual(MIN_CONTRAST);
    expect(shown.r).toBeGreaterThan(navy.r);
  });

  test("keeps a colour that already reads well", () => {
    const text = { r: 88, g: 110, b: 117 };
    expect(legible(text, cream)).toBe(text);
  });
});
