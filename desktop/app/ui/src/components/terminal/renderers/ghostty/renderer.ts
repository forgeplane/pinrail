// Draws libghostty-vt's render state on a canvas: one per terminal, at the
// device's pixel ratio, row by row, and only the rows that changed. Text
// goes in runs of cells with the same style, each glyph placed in its own
// cell; a grapheme cluster wider than a cell, an emoji or another glyph the
// font falls back for is drawn once into a cache and copied from there; box
// drawing, blocks, Braille and the Powerline separators are drawn from the
// cell's edges (glyphs.ts). The cursor is an element over the canvas, so
// it blinks without a redraw, and an idle terminal draws no frames.

import type { LinkSpan } from "./links";
import { drawGlyph, isDrawn, lightWidth, type Box } from "./glyphs";
import { ICON_FAMILY, iconFontReady, isIcon } from "./icons";
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
  /** text: plain glyphs in one style; glyph: one wide or other glyph,
   *  drawn by the font in its cells; drawn: by glyphs.ts; cached: an emoji,
   *  from the glyph cache; icon: a private-use icon, from the glyph cache
   *  in the symbols font; blank: nothing to draw */
  kind: "text" | "glyph" | "drawn" | "cached" | "icon" | "blank";
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

/** Colour emoji, which WebKit draws slowly, and which ignore the fill
 *  colour: flags, and pictographs shown as emoji. */
const EMOJI = /\p{Extended_Pictographic}|\p{Regional_Indicator}/u;

/** The kind of drawing a cell needs. */
export function cellKind(cell: GhosttyCell): Run["kind"] {
  if (!cell.text || cell.text === " " || cell.invisible) return "blank";
  if (isDrawn(cell.text)) return "drawn";
  if (isIcon(cell.text)) return "icon";
  if (plain(cell) && cell.wide === 0) return "text";
  return EMOJI.test(cell.text) ? "cached" : "glyph";
}

/** How many cells a glyph spans: two for a wide cluster, which the core
 *  marks with a spacer after it (GhosttyCellWide SPACER_TAIL). */
function spanOf(cells: readonly GhosttyCell[], at: number): number {
  return cells[at].wide === 1 || cells[at + 1]?.wide === 2 ? 2 : 1;
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

/** Characters drawn together in one string: letters, digits and the
 *  space, which no font joins into a ligature. */
const BATCHED = /^[A-Za-z0-9 ]$/;
/** A string whose width tells whether a font gives each of them the
 *  cell's width. */
const SAMPLE = "abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 mWil1";

/** Text the terminal's font usually lacks: CJK, Hangul, kana and emoji,
 *  which WebKit draws from fallback fonts. */
const FALLBACK_SAMPLE = "日本語中文測試한국어かなカナ🙂👍🏽";
/** The fonts whose fallbacks are found already, by CSS. */
const warmed = new Set<string>();

/** Finds the fallback fonts of a font's four styles in idle time. WebKit
 *  looks them up the first time it measures or draws such text, which
 *  took 50 to 90 ms in the first draw of a screen with CJK and emoji. */
function warmFallbacks(font: Font) {
  const fonts = [false, true].flatMap((bold) => [false, true].map((italic) => fontOf({ bold, italic }, font)));
  if (fonts.every((f) => warmed.has(f))) return;
  fonts.forEach((f) => warmed.add(f));
  const idle = window.requestIdleCallback ?? ((run: () => void) => setTimeout(run, 0));
  idle(() => {
    const ctx = document.createElement("canvas").getContext("2d");
    if (!ctx) return;
    for (const f of fonts) {
      ctx.font = f;
      ctx.measureText(FALLBACK_SAMPLE);
      ctx.fillText(FALLBACK_SAMPLE, 0, 10);
    }
  });
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
  /** Whether the canvas is in the viewport, and whether it has a size:
   *  none while it, or a parent, is out of the page or not displayed. */
  private inView = true;
  private laidOut = true;
  private observer: IntersectionObserver | null = null;
  private sizer: ResizeObserver | null = null;
  private shown: (() => void) | null = null;
  private redraw: (() => void) | null = null;
  private glyphs = new Map<string, HTMLCanvasElement>();
  /** How many rows the canvas holds drawn: none after a resize, which
   *  clears it, so the next draw draws them all. */
  private drawn = 0;
  /** What each row of the canvas shows, as a key of everything its
   *  drawing depends on: a row whose key is unchanged is not drawn again,
   *  and rows that moved are copied. */
  private keys: string[] = [];
  /** The link under the pointer, underlined, and the rows to draw again
   *  because it moved. */
  private link: readonly LinkSpan[] = [];
  private stale = new Set<number>();
  /** Find's matches in view, a mask of columns per row. */
  private matches = new Map<number, Uint8Array>();
  private findColour: GhosttyColor = { r: 120, g: 90, b: 0 };
  /** Whether each font, by its CSS, draws the batched characters at the
   *  cell's width, so that a string of them lands on the grid. */
  private even = new Map<string, boolean>();
  /** The cursor as last placed, to tell whether it moved. */
  private cursorKey = "";

  /** With `reference`, for tests only, the renderer draws in the plainest
   *  way: every row of every snapshot, in full. Its pixels are what any
   *  faster way of drawing must reproduce. */
  constructor(
    host: HTMLElement,
    private font: Font,
    private selection: GhosttyColor,
    private readonly reference = false,
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
    warmFallbacks(font);
    // a terminal out of sight draws nothing
    if (typeof IntersectionObserver !== "undefined") {
      this.observer = new IntersectionObserver((entries) =>
        this.see(
          entries.some((e) => e.isIntersecting),
          this.laidOut,
        ),
      );
      this.observer.observe(this.canvas);
    }
    // a canvas laid out again is shown again: the resize observer says so
    // before that frame is painted, where the intersection observer says
    // so a frame later
    if (typeof ResizeObserver !== "undefined") {
      this.sizer = new ResizeObserver((entries) => {
        const sized = entries.some((e) => e.contentRect.width > 0 && e.contentRect.height > 0);
        this.see(sized && !this.laidOut ? true : this.inView, sized);
      });
      this.sizer.observe(this.canvas);
    }
  }

  private see(inView: boolean, laidOut: boolean) {
    const was = this.showing;
    this.inView = inView;
    this.laidOut = laidOut;
    if (this.showing && !was) this.shown?.();
  }

  get cell(): CellMetrics {
    return this.metrics;
  }

  /** Whether drawing now would show anything. */
  get showing(): boolean {
    return this.inView && this.laidOut && this.canvas.isConnected;
  }

  /** Calls `listener` when the canvas comes back into sight. */
  onShown(listener: () => void) {
    this.shown = listener;
  }

  setFont(font: Font) {
    this.font = font;
    this.metrics = measureCell(font);
    warmFallbacks(font);
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

  /** Underlines the cells of the link under the pointer, or none. */
  setLink(spans: readonly LinkSpan[]) {
    for (const span of [...this.link, ...spans]) this.stale.add(span.row);
    this.link = spans;
  }

  /** Highlights find's matches in view in the find colour. */
  setMatches(spans: readonly LinkSpan[], colour?: GhosttyColor) {
    if (colour) this.findColour = colour;
    for (const row of this.matches.keys()) this.stale.add(row);
    this.matches = new Map();
    for (const span of spans) {
      let mask = this.matches.get(span.row);
      if (!mask) this.matches.set(span.row, (mask = new Uint8Array(Math.max(this.cols, span.end))));
      mask.fill(1, span.start, span.end);
      this.stale.add(span.row);
    }
  }

  /** Draws every row on the next draw, as after a selection changes. */
  invalidate() {
    this.drawn = 0;
  }

  /** Whether drawing `snapshot` would change what is shown: a row or the
   *  cursor changed, rows wait to be drawn again, or the grid or the
   *  pixel ratio changed. */
  changes(snapshot: GhosttySnapshot, focused: boolean): boolean {
    return (
      snapshot.dirtyRows.size > 0 ||
      this.stale.size > 0 ||
      this.drawn !== snapshot.rows ||
      this.cols !== snapshot.cols ||
      this.ratio !== (window.devicePixelRatio || 1) ||
      this.cursorKey !== cursorKeyOf(snapshot, focused)
    );
  }

  /** Draws the rows of `snapshot` that changed, or all of them after a
   *  resize, and places the cursor. */
  draw(snapshot: GhosttySnapshot, focused: boolean) {
    this.resize(snapshot.cols, snapshot.rows);
    const all = this.reference || this.drawn !== snapshot.rows;
    if (all) {
      this.keys = [];
      snapshot.rowData.forEach((row, y) => {
        this.drawRow(row.cells, y, snapshot);
        if (!this.reference) this.keys[y] = this.rowKey(row.cells, y, snapshot);
      });
    } else {
      const changed = new Map<number, string>();
      snapshot.rowData.forEach((row, y) => {
        if (snapshot.dirtyRows.has(y) || this.stale.has(y)) changed.set(y, this.rowKey(row.cells, y, snapshot));
      });
      this.copyMoved(changed);
      for (const [y, key] of changed) {
        if (this.keys[y] === key) continue;
        this.drawRow(snapshot.rowData[y]!.cells, y, snapshot);
        this.keys[y] = key;
      }
    }
    this.stale.clear();
    this.drawn = snapshot.rows;
    this.placeCursor(snapshot, focused);
  }

  /** Copies the longest band of changed rows that the canvas already shows
   *  some rows lower, as when output scrolls up, so that only the new rows
   *  are drawn. Only where a row is a whole number of device pixels tall,
   *  so that the copy lands on the same pixels a drawing would. */
  private copyMoved(changed: ReadonlyMap<number, string>) {
    const h = this.metrics.height;
    if (changed.size < 2 || !Number.isInteger(h * this.ratio)) return;
    let best = { from: 0, to: 0, by: 0 };
    for (const [y, key] of changed) {
      if (this.keys[y] === key) continue;
      const by = this.keys.indexOf(key, y + 1) - y;
      if (by <= 0) continue;
      let end = y + 1;
      while (end + by < this.rows && changed.get(end) === this.keys[end + by] && changed.has(end)) end++;
      if (end - y > best.to - best.from) best = { from: y, to: end, by };
    }
    const count = best.to - best.from;
    if (count < 2) return;
    const px = h * this.ratio;
    this.context.drawImage(
      this.canvas,
      0,
      (best.from + best.by) * px,
      this.canvas.width,
      count * px,
      0,
      best.from * h,
      this.canvas.width / this.ratio,
      count * h,
    );
    for (let y = best.from; y < best.to; y++) this.keys[y] = changed.get(y)!;
  }

  /** A key of everything a row's drawing depends on. */
  private rowKey(cells: readonly GhosttyCell[], y: number, snapshot: GhosttySnapshot): string {
    let a = 0x811c9dc5;
    let b = 0x01000193;
    const mix = (n: number) => {
      a = Math.imul(a ^ n, 0x01000193);
      b = Math.imul(b ^ n, 0x5bd1e995) + 0x6b43a9b5;
    };
    const colour = (c: GhosttyColor) => mix((c.r << 16) | (c.g << 8) | c.b);
    colour(snapshot.background);
    colour(this.selection);
    colour(this.findColour);
    for (const cell of cells) {
      for (let i = 0; i < cell.text.length; i++) mix(cell.text.charCodeAt(i));
      mix(
        cell.text.length |
          (cell.wide << 8) |
          (cell.bold ? 1 << 10 : 0) |
          (cell.italic ? 1 << 11 : 0) |
          (cell.invisible ? 1 << 12 : 0) |
          (cell.strikethrough ? 1 << 13 : 0) |
          (cell.overline ? 1 << 14 : 0) |
          (cell.underline << 15) |
          (cell.selected ? 1 << 18 : 0),
      );
      colour(cell.foreground);
      colour(cell.background);
      if (cell.underlineColor) colour(cell.underlineColor);
    }
    const found = this.matches.get(y);
    if (found) for (let x = 0; x < found.length; x++) if (found[x]) mix(0x40000 + x);
    for (const span of this.link) if (span.row === y) mix(0x80000 + span.start * 1024 + span.end);
    return `${a >>> 0}:${b >>> 0}`;
  }

  private drawRow(cells: readonly GhosttyCell[], y: number, snapshot: GhosttySnapshot) {
    const ctx = this.context;
    const { width: w, height: h, baseline } = this.metrics;
    const top = y * h;
    // the backgrounds, a fill per run of one colour
    ctx.fillStyle = css(snapshot.background);
    // to the canvas's last device pixel, so that a row drawn again is
    // drawn the same: a fill that ended inside a pixel would blend into it
    // once more each time
    ctx.fillRect(0, top, this.canvas.width / this.ratio, h);
    const found = this.matches.get(y);
    const backgroundOf = (x: number) =>
      cells[x].selected ? this.selection : found?.[x] ? this.findColour : cells[x].background;
    for (let x = 0; x < cells.length;) {
      const bg = backgroundOf(x);
      let end = x + 1;
      while (end < cells.length && same(backgroundOf(end), bg)) end++;
      if (!same(bg, snapshot.background)) {
        ctx.fillStyle = css(bg);
        ctx.fillRect(x * w, top, (end - x) * w, h);
      }
      x = end;
    }
    // the text, each run in its own way; letters, digits and the spaces
    // between them go out as one string of one style
    let batch: { start: number; text: string; cell: GhosttyCell; spaces: number } | null = null;
    const flush = () => {
      if (batch) ctx.fillText(batch.text, batch.start * w, top + baseline);
      batch = null;
    };
    for (const run of splitRuns(cells)) {
      const first = cells[run.start];
      if (run.kind === "blank") {
        if (batch) batch.spaces += run.end - run.start;
        continue;
      }
      if (run.kind !== "text") flush();
      if (run.kind === "text") {
        const font = fontOf(first, this.font);
        const colour = css(first.foreground);
        // the reference draws every glyph alone
        const even = !this.reference && !first.bold && !first.italic && this.isEven(font);
        for (let x = run.start; x < run.end; x++) {
          const text = cells[x].text;
          if (even && BATCHED.test(text)) {
            if (batch && sameText(batch.cell, cells[x])) {
              batch.text += " ".repeat(batch.spaces) + text;
              batch.spaces = 0;
              continue;
            }
            flush();
            ctx.font = font;
            ctx.fillStyle = colour;
            batch = { start: x, text, cell: cells[x], spaces: 0 };
            continue;
          }
          // each other glyph in its own cell, as WebKit has no letter spacing
          flush();
          ctx.font = font;
          ctx.fillStyle = colour;
          ctx.fillText(text, x * w, top + baseline);
        }
      } else if (run.kind === "glyph") {
        // a wide or other glyph, centred in its cells by the font, and
        // squeezed into them if it is wider
        const span = spanOf(cells, run.start);
        ctx.font = fontOf(first, this.font);
        ctx.fillStyle = css(first.foreground);
        const width = ctx.measureText(first.text).width;
        ctx.fillText(first.text, run.start * w + Math.max(0, (span * w - width) / 2), top + baseline, span * w);
      } else if (run.kind === "drawn") {
        ctx.fillStyle = css(first.foreground);
        drawGlyph(ctx, first.text, { x: run.start * w, y: top, w, h });
      } else if (run.kind === "cached") {
        const span = spanOf(cells, run.start);
        ctx.drawImage(this.glyph(first, span), run.start * w, top, span * w, h);
      } else if (run.kind === "icon") {
        // drawn once the symbols font is in; every row is drawn again then
        if (!iconFontReady(this.iconFontArrived)) continue;
        // two cells of room when a blank cell follows, as in prompts and
        // file trees, else its own
        const next = cells[run.start + 1];
        const room = next && cellKind(next) === "blank" && next.wide !== 2 ? 2 : 1;
        ctx.drawImage(this.icon(first, room), run.start * w, top, room * w, h);
      }
    }
    flush();
    // underlines, strikethrough and overline
    for (let x = 0; x < cells.length; x++) {
      const cell = cells[x];
      if (cell.underline || cell.strikethrough || cell.overline) {
        this.decorate(cell, { x: x * w, y: top, w, h });
      }
    }
    // the link under the pointer
    for (const span of this.link) {
      if (span.row !== y) continue;
      for (let x = span.start; x < span.end && x < cells.length; x++) {
        if (!cells[x].underline) this.decorate({ ...cells[x], underline: 1 }, { x: x * w, y: top, w, h });
      }
    }
  }

  /** Whether `font` gives each batched character the cell's width. */
  private isEven(font: string): boolean {
    let even = this.even.get(font);
    if (even === undefined) {
      this.context.font = font;
      const width = this.context.measureText(SAMPLE).width;
      even = Math.abs(width - SAMPLE.length * this.metrics.width) < 0.5;
      this.even.set(font, even);
    }
    return even;
  }

  /** A glyph drawn once into the cache, in its cells' size: emoji and
   *  other fallback glyphs are slow to draw in WebKit. */
  private glyph(cell: GhosttyCell, span: number): HTMLCanvasElement {
    const { width: w, height: h, baseline } = this.metrics;
    // an emoji's own colours are drawn whatever the fill colour
    const key = `${cell.text}|${fontOf(cell, this.font)}|${span}`;
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

  /** An icon drawn once into the cache, in `room` cells. It is scaled as a
   *  whole by its outline, to 75% of two cells' width or 90% of one, and
   *  72% of the cell's height, then centred in the line and in its own
   *  cell; a wider icon starts at its cell and spills into the blank cell
   *  after it. The terminal's font comes first, so a patched Nerd Font
   *  keeps its own icons. */
  private icon(cell: GhosttyCell, room: number): HTMLCanvasElement {
    const { width: w, height: h } = this.metrics;
    const family = `${this.font.family}, "${ICON_FAMILY}"`;
    const key = `icon|${cell.text}|${family}|${room}|${css(cell.foreground)}`;
    const kept = this.glyphs.get(key);
    if (kept) return kept;
    if (this.glyphs.size >= CACHE_LIMIT) this.glyphs.clear();
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(room * w * this.ratio);
    canvas.height = Math.round(h * this.ratio);
    const ctx = canvas.getContext("2d")!;
    ctx.scale(this.ratio, this.ratio);
    ctx.fillStyle = css(cell.foreground);
    ctx.font = `${this.font.size}px ${family}`;
    const m = ctx.measureText(cell.text);
    const [left, right] = [m.actualBoundingBoxLeft, m.actualBoundingBoxRight];
    const [ascent, descent] = [m.actualBoundingBoxAscent, m.actualBoundingBoxDescent];
    const [gw, gh] = [left + right, ascent + descent];
    if (gw > 0 && gh > 0) {
      const scale = Math.min(((room === 2 ? 0.75 : 0.9) * room * w) / gw, (0.72 * h) / gh);
      ctx.font = `${this.font.size * scale}px ${family}`;
      const x = Math.max(0, (w - gw * scale) / 2) + left * scale;
      const y = (h - gh * scale) / 2 + ascent * scale;
      ctx.fillText(cell.text, x, y);
    }
    this.glyphs.set(key, canvas);
    return canvas;
  }

  /** The symbols font is in: every row is drawn again, its icons too. */
  private iconFontArrived = () => {
    this.invalidate();
    this.redraw?.();
  };

  /** Asks for a draw, as when the symbols font arrives. */
  onRedraw(listener: () => void) {
    this.redraw = listener;
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
    this.cursorKey = cursorKeyOf(snapshot, focused);
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
    this.sizer?.disconnect();
    this.glyphs.clear();
    this.canvas.remove();
    this.cursor.remove();
  }
}

/** What the cursor's look depends on, as text. */
function cursorKeyOf(s: GhosttySnapshot, focused: boolean): string {
  return `${s.cursorX},${s.cursorY},${s.cursorVisible},${s.cursorStyle},${s.cursorBlinking},${css(s.cursor)},${focused}`;
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
