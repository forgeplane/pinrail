// Draws libghostty-vt's render state on a canvas: one per terminal, at the
// device's pixel ratio, row by row, and only the rows that changed. Text
// goes in runs of cells with the same style, each glyph placed in its own
// cell; a grapheme cluster wider than a cell, an emoji or another glyph the
// font falls back for is drawn once into a cache and copied from there; box
// drawing, blocks, Braille and the Powerline separators are drawn from the
// cell's edges (glyphs.ts). The cursor is an element over the canvas, so
// it blinks without a redraw, and an idle terminal draws no frames.

import { drawGlyph, isDrawn, lightWidth, type Box } from "./glyphs";
import type { GhosttyCell, GhosttyColor, GhosttySnapshot } from "./types";

/** The size of a cell, and where text sits in it, in CSS pixels. */
export interface CellMetrics {
  width: number;
  height: number;
  baseline: number;
}

export interface Font {
  family: string;
  size: number;
  lineHeight: number;
}

/** A stretch of a row drawn one way. */
export interface Run {
  start: number;
  end: number;
  /** text: plain glyphs in one style; drawn: by glyphs.ts; cached: from
   *  the glyph cache; blank: nothing to draw */
  kind: "text" | "drawn" | "cached" | "blank";
}

const css = ({ r, g, b }: GhosttyColor) => `rgb(${r},${g},${b})`;
const same = (a: GhosttyColor, b: GhosttyColor) => a.r === b.r && a.g === b.g && a.b === b.b;

/** Whether a cell's text is a plain glyph the font draws in one cell:
 *  one code point, narrow, below the emoji and the private use area. */
function plain(cell: GhosttyCell): boolean {
  const text = cell.text;
  if (text.length !== 1) return false;
  const cp = text.charCodeAt(0);
  return cp < 0x2e80 && !(cp >= 0xd800 && cp <= 0xdfff);
}

/** The kind of drawing a cell needs. */
export function cellKind(cell: GhosttyCell): Run["kind"] {
  if (!cell.text || cell.text === " " || cell.invisible) return "blank";
  if (isDrawn(cell.text)) return "drawn";
  return plain(cell) && cell.wide === 0 ? "text" : "cached";
}

/** Whether two text cells share a font and a colour. */
function sameText(a: GhosttyCell, b: GhosttyCell) {
  return a.bold === b.bold && a.italic === b.italic && same(a.foreground, b.foreground);
}

/** Splits a row into runs: text cells with the same style together, every
 *  other cell alone, and blanks together. */
export function splitRuns(cells: readonly GhosttyCell[]): Run[] {
  const runs: Run[] = [];
  for (let i = 0; i < cells.length; i++) {
    const kind = cellKind(cells[i]);
    const last = runs[runs.length - 1];
    const joins =
      last &&
      last.end === i &&
      last.kind === kind &&
      (kind === "blank" || (kind === "text" && sameText(cells[last.start], cells[i])));
    if (joins) last.end = i + 1;
    else runs.push({ start: i, end: i + 1, kind });
  }
  return runs;
}

/** The CSS font of a cell. */
function fontOf(cell: Pick<GhosttyCell, "bold" | "italic">, font: Font) {
  return `${cell.italic ? "italic " : ""}${cell.bold ? "bold " : ""}${font.size}px ${font.family}`;
}

/** Measures the cell of a font: the advance of its glyphs, the line
 *  height, and the baseline. */
export function measureCell(font: Font): CellMetrics {
  const context = document.createElement("canvas").getContext("2d")!;
  context.font = fontOf({ bold: false, italic: false }, font);
  const m = context.measureText("W");
  const height = Math.ceil(font.size * font.lineHeight);
  const ascent = m.fontBoundingBoxAscent || font.size * 0.8;
  const descent = m.fontBoundingBoxDescent || font.size * 0.2;
  // the glyphs sit in the middle of the line
  const baseline = Math.round((height - (ascent + descent)) / 2 + ascent);
  return { width: m.width, height, baseline };
}

/** How many glyphs the cache keeps before it starts afresh. */
const CACHE_LIMIT = 1024;

export class CanvasRenderer {
  readonly canvas: HTMLCanvasElement;
  readonly cursor: HTMLDivElement;
  private context: CanvasRenderingContext2D;
  private metrics: CellMetrics;
  private cols = 0;
  private rows = 0;
  private ratio = 1;
  private visible = true;
  private observer: IntersectionObserver | null = null;
  private glyphs = new Map<string, HTMLCanvasElement>();
  /** How many rows the canvas holds drawn: none after a resize, which
   *  clears it, so the next draw draws them all. */
  private drawn = 0;

  constructor(
    host: HTMLElement,
    private font: Font,
    private selection: GhosttyColor,
  ) {
    this.canvas = document.createElement("canvas");
    this.canvas.className = "ghostty-canvas";
    this.canvas.setAttribute("aria-hidden", "true");
    this.cursor = document.createElement("div");
    this.cursor.className = "ghostty-cursor";
    this.cursor.setAttribute("aria-hidden", "true");
    host.append(this.canvas, this.cursor);
    this.context = this.canvas.getContext("2d", { alpha: false })!;
    this.metrics = measureCell(font);
    // a terminal out of sight draws nothing
    if (typeof IntersectionObserver !== "undefined") {
      this.observer = new IntersectionObserver((entries) => {
        this.visible = entries.some((e) => e.isIntersecting);
      });
      this.observer.observe(this.canvas);
    }
  }

  get cell(): CellMetrics {
    return this.metrics;
  }

  /** Whether drawing now would show anything. */
  get showing(): boolean {
    return this.visible && this.canvas.isConnected;
  }

  setFont(font: Font) {
    this.font = font;
    this.metrics = measureCell(font);
    this.glyphs.clear();
    this.resize(this.cols, this.rows, true);
  }

  setSelectionColour(colour: GhosttyColor) {
    this.selection = colour;
  }

  /** Sizes the canvas to the grid at the device's pixel ratio. */
  resize(cols: number, rows: number, force = false) {
    const ratio = window.devicePixelRatio || 1;
    if (!force && cols === this.cols && rows === this.rows && ratio === this.ratio) return;
    this.cols = cols;
    this.rows = rows;
    this.ratio = ratio;
    const { width, height } = this.metrics;
    this.canvas.style.width = `${cols * width}px`;
    this.canvas.style.height = `${rows * height}px`;
    this.canvas.width = Math.round(cols * width * ratio);
    this.canvas.height = Math.round(rows * height * ratio);
    this.context.setTransform(ratio, 0, 0, ratio, 0, 0);
    this.context.textBaseline = "alphabetic";
    this.glyphs.clear();
    this.drawn = 0;
  }

  /** Draws the rows of `snapshot` that changed, or all of them after a
   *  resize, and places the cursor. */
  draw(snapshot: GhosttySnapshot, focused: boolean) {
    this.resize(snapshot.cols, snapshot.rows);
    const all = this.drawn !== snapshot.rows;
    snapshot.rowData.forEach((row, y) => {
      if (all || snapshot.dirtyRows.has(y)) this.drawRow(row.cells, y, snapshot);
    });
    this.drawn = snapshot.rows;
    this.placeCursor(snapshot, focused);
  }

  private drawRow(cells: readonly GhosttyCell[], y: number, snapshot: GhosttySnapshot) {
    const ctx = this.context;
    const { width: w, height: h, baseline } = this.metrics;
    const top = y * h;
    // the backgrounds, a fill per run of one colour
    ctx.fillStyle = css(snapshot.background);
    ctx.fillRect(0, top, this.cols * w, h);
    for (let x = 0; x < cells.length;) {
      const bg = cells[x].selected ? this.selection : cells[x].background;
      let end = x + 1;
      while (end < cells.length && same(cells[end].selected ? this.selection : cells[end].background, bg)) end++;
      if (!same(bg, snapshot.background)) {
        ctx.fillStyle = css(bg);
        ctx.fillRect(x * w, top, (end - x) * w, h);
      }
      x = end;
    }
    // the text, each run in its own way
    for (const run of splitRuns(cells)) {
      const first = cells[run.start];
      if (run.kind === "text") {
        ctx.font = fontOf(first, this.font);
        ctx.fillStyle = css(first.foreground);
        // each glyph in its own cell, as WebKit has no letter spacing
        for (let x = run.start; x < run.end; x++) ctx.fillText(cells[x].text, x * w, top + baseline);
      } else if (run.kind === "drawn") {
        ctx.fillStyle = css(first.foreground);
        drawGlyph(ctx, first.text, { x: run.start * w, y: top, w, h });
      } else if (run.kind === "cached") {
        // a wide cluster spans the cell after it too, which the core
        // reads as its spacer (GhosttyCellWide SPACER_TAIL)
        const span = first.wide === 1 || cells[run.start + 1]?.wide === 2 ? 2 : 1;
        ctx.drawImage(this.glyph(first, span), run.start * w, top, span * w, h);
      }
    }
    // underlines, strikethrough and overline
    for (let x = 0; x < cells.length; x++) {
      const cell = cells[x];
      if (cell.underline || cell.strikethrough || cell.overline) {
        this.decorate(cell, { x: x * w, y: top, w, h });
      }
    }
  }

  /** A glyph drawn once into the cache, in its cells' size: emoji and
   *  other fallback glyphs are slow to draw in WebKit. */
  private glyph(cell: GhosttyCell, span: number): HTMLCanvasElement {
    const { width: w, height: h, baseline } = this.metrics;
    const key = `${cell.text}|${fontOf(cell, this.font)}|${css(cell.foreground)}|${span}`;
    const kept = this.glyphs.get(key);
    if (kept) return kept;
    if (this.glyphs.size >= CACHE_LIMIT) this.glyphs.clear();
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(span * w * this.ratio);
    canvas.height = Math.round(h * this.ratio);
    const ctx = canvas.getContext("2d")!;
    ctx.scale(this.ratio, this.ratio);
    ctx.font = fontOf(cell, this.font);
    ctx.fillStyle = css(cell.foreground);
    // the whole cluster as one glyph, centred in its cells
    const width = ctx.measureText(cell.text).width;
    ctx.fillText(cell.text, Math.max(0, (span * w - width) / 2), baseline, span * w);
    this.glyphs.set(key, canvas);
    return canvas;
  }

  /** A cell's lines: the underline in its style and colour, the
   *  strikethrough and the overline. */
  private decorate(cell: GhosttyCell, box: Box) {
    const ctx = this.context;
    const line = lightWidth(box);
    const { x, y, w, h } = box;
    ctx.fillStyle = css(cell.foreground);
    if (cell.strikethrough) ctx.fillRect(x, Math.round(y + h / 2), w, line);
    if (cell.overline) ctx.fillRect(x, y, w, line);
    if (!cell.underline) return;
    ctx.fillStyle = css(cell.underlineColor ?? cell.foreground);
    ctx.strokeStyle = ctx.fillStyle;
    const under = Math.min(y + h - line, y + this.metrics.baseline + line * 2);
    switch (cell.underline) {
      case 2: // double
        ctx.fillRect(x, under - line * 2, w, line);
        ctx.fillRect(x, under, w, line);
        break;
      case 3: {
        // curly: a wave of a cell's width
        ctx.lineWidth = line;
        ctx.beginPath();
        ctx.moveTo(x, under);
        ctx.quadraticCurveTo(x + w / 4, under - line * 2, x + w / 2, under);
        ctx.quadraticCurveTo(x + (w * 3) / 4, under + line * 2, x + w, under);
        ctx.stroke();
        break;
      }
      case 4: // dotted
        for (let dx = 0; dx < w; dx += line * 2) ctx.fillRect(x + dx, under, line, line);
        break;
      case 5: // dashed
        ctx.fillRect(x, under, w * 0.6, line);
        break;
      default:
        ctx.fillRect(x, under, w, line);
    }
  }

  /** The cursor element: a block, bar or underline over its cell, hollow
   *  when the terminal does not have the keyboard, blinking by CSS. */
  private placeCursor(snapshot: GhosttySnapshot, focused: boolean) {
    const el = this.cursor;
    const show = snapshot.cursorVisible && snapshot.cursorY >= 0 && snapshot.cursorX >= 0;
    el.hidden = !show;
    if (!show) return;
    const { width: w, height: h } = this.metrics;
    // GhosttyCursorStyle: bar 0, block 1, underline 2, hollow block 3
    const style = ["bar", "block", "underline", "block"][snapshot.cursorStyle] ?? "block";
    const hollow = !focused || snapshot.cursorStyle === 3;
    el.className = `ghostty-cursor is-${style}${hollow ? " is-hollow" : ""}${
      snapshot.cursorBlinking && focused ? " is-blinking" : ""
    }`;
    el.style.left = `${snapshot.cursorX * w}px`;
    el.style.top = `${snapshot.cursorY * h}px`;
    el.style.width = `${w}px`;
    el.style.height = `${h}px`;
    el.style.setProperty("--cursor-colour", css(snapshot.cursor));
  }

  dispose() {
    this.observer?.disconnect();
    this.glyphs.clear();
    this.canvas.remove();
    this.cursor.remove();
  }
}

/** The frame clock all of a window's terminals share: one animation frame
 *  draws every terminal that asked, and none draws while nothing asks. */
const asked = new Set<() => void>();
let frame = 0;

/** Draws on the next frame, with every other terminal that asks. */
export function onNextFrame(draw: () => void) {
  asked.add(draw);
  if (frame) return;
  frame = requestAnimationFrame(() => {
    frame = 0;
    const now = [...asked];
    asked.clear();
    for (const run of now) run();
  });
}
