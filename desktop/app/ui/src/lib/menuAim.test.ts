import { describe, expect, it } from "vitest";
import { headsFor } from "./menuAim";

// a submenu to the right of its menu, from x 200 to 360 and y 100 to 300
const right = { left: 200, right: 360, top: 100, bottom: 300 };
// the same submenu opening to the left, from x 0 to 160
const left = { left: 0, right: 160, top: 100, bottom: 300 };

describe("headsFor", () => {
  it("is true for a move down and toward a submenu on the right", () => {
    expect(headsFor({ x: 150, y: 110 }, { x: 160, y: 125 }, right)).toBe(true);
  });

  it("is true for a move up and toward it", () => {
    expect(headsFor({ x: 150, y: 290 }, { x: 160, y: 275 }, right)).toBe(true);
  });

  it("is true for a submenu that opens on the left", () => {
    expect(headsFor({ x: 210, y: 110 }, { x: 200, y: 125 }, left)).toBe(true);
  });

  it("is false for a move straight down, along the menu", () => {
    expect(headsFor({ x: 150, y: 110 }, { x: 150, y: 140 }, right)).toBe(false);
  });

  it("is false for a move away from the submenu", () => {
    expect(headsFor({ x: 150, y: 110 }, { x: 140, y: 125 }, right)).toBe(false);
  });

  it("is false for a move steeper than the line to the submenu's far corner", () => {
    // from (150, 250), the bottom corner (200, 300) lies at 45°; this goes at 60°
    expect(headsFor({ x: 150, y: 250 }, { x: 160, y: 270 }, right)).toBe(false);
  });

  it("is false for a pointer that has not moved", () => {
    expect(headsFor({ x: 150, y: 110 }, { x: 150, y: 110 }, right)).toBe(false);
  });

  it("is false once the pointer is level with the submenu or past its edge", () => {
    expect(headsFor({ x: 190, y: 110 }, { x: 205, y: 120 }, right)).toBe(false);
  });
});
