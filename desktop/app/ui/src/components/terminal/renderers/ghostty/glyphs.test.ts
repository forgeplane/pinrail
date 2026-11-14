import { describe, expect, test } from "vitest";
import { drawGlyph, isDrawn, type Pen } from "./glyphs";

/** A pen that records what it draws. */
function pen() {
  const drawn: string[] = [];
  const p: Pen & { drawn: string[] } = {
    drawn,
    fillStyle: "#fff",
    strokeStyle: "#fff",
    lineWidth: 1,
    globalAlpha: 1,
    fillRect: (x, y, w, h) => drawn.push(`rect ${x} ${y} ${w} ${h} @${p.globalAlpha}`),
    beginPath: () => drawn.push("path"),
    moveTo: (x, y) => drawn.push(`move ${x} ${y}`),
    lineTo: (x, y) => drawn.push(`line ${x} ${y}`),
    quadraticCurveTo: (_cx, _cy, x, y) => drawn.push(`curve ${x} ${y}`),
    arc: (x, y) => drawn.push(`arc ${x} ${y}`),
    closePath: () => drawn.push("close"),
    fill: () => drawn.push("fill"),
    stroke: () => drawn.push("stroke"),
  };
  return p;
}

// a cell of 10 by 20 pixels, whose light lines are 1 pixel thick
const box = { x: 0, y: 0, w: 10, h: 20 };

describe("the glyphs the renderer draws", () => {
  test("are the box drawing, block, Braille and Powerline characters", () => {
    for (const text of ["─", "╋", "╭", "█", "▚", "⠿", ""]) expect(isDrawn(text), text).toBe(true);
    for (const text of ["a", "", "─a", "😀", ""]) expect(isDrawn(text), text).toBe(false);
  });

  test("a light line runs from edge to edge through the middle", () => {
    const p = pen();
    drawGlyph(p, "─", box);
    // its two arms meet in the middle
    expect(p.drawn).toEqual(["rect 0 10 5 1 @1", "rect 5 10 5 1 @1"]);
    const v = pen();
    drawGlyph(v, "│", box);
    expect(v.drawn).toEqual(["rect 5 0 1 10 @1", "rect 5 10 1 10 @1"]);
  });

  test("a heavy line is twice as thick, and a double line is two lines", () => {
    const heavy = pen();
    drawGlyph(heavy, "━", box);
    expect(heavy.drawn.every((d) => d.endsWith(" 2 @1"))).toBe(true);
    const double = pen();
    drawGlyph(double, "═", box);
    expect(double.drawn).toHaveLength(4);
  });

  test("a junction draws each of its arms", () => {
    const p = pen();
    drawGlyph(p, "┼", box);
    expect(p.drawn).toHaveLength(4);
    // a corner's arms end at each other's far edge: no gap, no stub
    const corner = pen();
    drawGlyph(corner, "┌", box);
    expect(corner.drawn).toEqual(["rect 5 10 1 10 @1", "rect 5 10 5 1 @1"]);
  });

  test("blocks fill their part of the cell, and shades their share of it", () => {
    const upper = pen();
    drawGlyph(upper, "▀", box);
    expect(upper.drawn).toEqual(["rect 0 0 10 10 @1"]);
    const lower = pen();
    drawGlyph(lower, "▄", box);
    expect(lower.drawn).toEqual(["rect 0 10 10 10 @1"]);
    const shade = pen();
    drawGlyph(shade, "▒", box);
    expect(shade.drawn).toEqual(["rect 0 0 10 20 @0.5"]);
    expect(shade.globalAlpha).toBe(1);
    const quadrants = pen();
    drawGlyph(quadrants, "▚", box);
    expect(quadrants.drawn).toEqual(["rect 0 0 5 10 @1", "rect 5 10 5 10 @1"]);
  });

  test("Braille draws a dot for each bit, in two columns of four", () => {
    const one = pen();
    drawGlyph(one, "⠁", box);
    expect(one.drawn.filter((d) => d.startsWith("arc"))).toEqual(["arc 2.5 2.5"]);
    const all = pen();
    drawGlyph(all, "⣿", box);
    expect(all.drawn.filter((d) => d.startsWith("arc"))).toHaveLength(8);
  });

  test("the Powerline arrows fill or stroke a triangle to the cell's far edge", () => {
    const solid = pen();
    drawGlyph(solid, "", box);
    expect(solid.drawn).toEqual(["path", "move 0 0", "line 10 10", "line 0 20", "close", "fill"]);
    const thin = pen();
    drawGlyph(thin, "", box);
    expect(thin.drawn).toEqual(["path", "move 10 0", "line 0 10", "line 10 20", "stroke"]);
  });
});
