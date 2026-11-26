// The pointer over a libghostty terminal. When the program turns on a
// mouse mode, clicks, drags and the wheel go to libghostty's mouse encoder;
// holding Shift selects text instead. Otherwise the pointer selects text,
// which libghostty holds in screen coordinates so that it survives
// scrolling: a drag selects cells, Option-drag a rectangle, a double-click
// a word and a triple-click a line, and Shift-click extends the selection.
// Dragging past the top or bottom edge scrolls. The wheel scrolls the
// scrollback, or sends arrow keys on the alternate screen. Links under the
// pointer are underlined, and Cmd-click (Ctrl-click on Linux) opens them.

import type { InputKind } from "../../view";
import type { GhosttyCore } from "./core";
import { hyperlinkAt, type Link, type LinkSpan, urlAt } from "./links";
import type { CellMetrics } from "./renderer";
import type { GhosttyRow } from "./types";

/** What the pointer needs of the terminal. */
export type PointerTarget = Pick<
  GhosttyCore,
  | "clearSelection"
  | "encodeKey"
  | "encodeMouse"
  | "hyperlinkAt"
  | "isAlternateScreen"
  | "isMouseTracking"
  | "scroll"
  | "scrollbar"
  | "selectLine"
  | "selectWord"
  | "setSelection"
>;

export interface PointerOptions {
  mac: boolean;
  /** The size of a cell, in CSS pixels. */
  cell(): CellMetrics;
  /** The visible rows, as last drawn. */
  rows(): readonly GhosttyRow[];
  emit(bytes: Uint8Array, kind: InputKind): void;
  /** Draws the terminal again, every row, after the selection changed. */
  redraw(): void;
  /** Draws the terminal on the next frame after the wheel moved the
   *  viewport. The rows that moved are copied rather than drawn. */
  scrolled(): void;
  /** Underlines the link under the pointer, or none. */
  showLink(spans: readonly LinkSpan[]): void;
  onLink(uri: string, event: MouseEvent): void;
  /** Gives the terminal the keyboard. */
  focus(): void;
}

interface Cell {
  col: number;
  row: number;
}

/** How long the pointer rests on a row before its plain URLs are looked
 *  for. */
const REST_MS = 80;
/** How often a drag past an edge scrolls. */
const AUTOSCROLL_MS = 50;

/** GhosttyMods bits from an event's modifier keys. */
export function modsOf(e: MouseEvent | KeyboardEvent): number {
  return (e.shiftKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.altKey ? 4 : 0) | (e.metaKey ? 8 : 0);
}

/** GhosttyMouseButton for a DOM button: left, middle and right. */
export function buttonOf(button: number): number | null {
  return ({ 0: 1, 1: 3, 2: 2 } as Record<number, number>)[button] ?? null;
}

/**
 * Whole rows to scroll for a wheel event, and the pixels left over, which
 * the next event adds to. Negative rows scroll up, into the scrollback.
 */
export function wheelRows(
  pending: number,
  event: Pick<WheelEvent, "deltaY" | "deltaMode">,
  rowHeight: number,
  rows: number,
) {
  const unit = event.deltaMode === 1 ? rowHeight : event.deltaMode === 2 ? rowHeight * rows : 1;
  const total = pending + event.deltaY * unit;
  const whole = Math.trunc(total / rowHeight);
  return { rows: whole, pending: total - whole * rowHeight };
}

/** Rows to scroll each tick of a drag past an edge: more the further the
 *  pointer is past it. */
export function autoscrollRows(distance: number, rowHeight: number): number {
  return Math.min(10, 1 + Math.floor(distance / Math.max(1, rowHeight * 2)));
}

export class Pointer {
  private cleanup: (() => void)[] = [];
  /** The selection's fixed end, in screen rows, while or after a drag. */
  private anchor: Cell | null = null;
  private dragging = false;
  private moved = false;
  private rectangle = false;
  /** Whether the program has the button that is down. */
  private reporting = false;
  private lastReported: Cell | null = null;
  private autoscroll = 0;
  private autoscrollBy = 0;
  private lastPointer: { x: number; y: number } | null = null;
  private wheelPending = 0;
  private hovered: Cell | null = null;
  private link: Link | null = null;
  private rest = 0;

  constructor(
    private readonly element: HTMLElement,
    private readonly target: PointerTarget,
    private readonly options: PointerOptions,
  ) {
    this.on("pointerdown", (e) => this.down(e as PointerEvent));
    this.on("pointermove", (e) => this.move(e as PointerEvent));
    this.on("pointerup", (e) => this.up(e as PointerEvent));
    this.on("pointercancel", () => this.stopDrag());
    this.on("pointerleave", () => this.hover(null));
    this.on("wheel", (e) => this.wheel(e as WheelEvent), { passive: false });
    this.on("contextmenu", (e) => {
      if (this.target.isMouseTracking() && !(e as MouseEvent).shiftKey) e.preventDefault();
    });
  }

  private on(type: string, listener: (event: Event) => void, options?: AddEventListenerOptions) {
    this.element.addEventListener(type, listener, options);
    this.cleanup.push(() => this.element.removeEventListener(type, listener, options));
  }

  /** The cell under a point, clamped to the grid. */
  private cellAt(x: number, y: number): Cell {
    const rect = this.element.getBoundingClientRect();
    const { width, height } = this.options.cell();
    const rows = this.options.rows();
    const cols = rows[0]?.cells.length ?? 1;
    const clamp = (v: number, max: number) => Math.min(Math.max(v, 0), Math.max(max - 1, 0));
    return {
      col: clamp(Math.floor((x - rect.left) / width), cols),
      row: clamp(Math.floor((y - rect.top) / height), rows.length),
    };
  }

  private toScreen(cell: Cell): Cell {
    return { col: cell.col, row: cell.row + this.target.scrollbar().offset };
  }

  /** Keeps getting the pointer's events while a button is down, outside
   *  the terminal too. */
  private capture(pointerId: number) {
    try {
      this.element.setPointerCapture(pointerId);
    } catch {
      // a pointer that is no longer down, such as one a test made up
    }
  }

  /** Whether the program gets this event rather than the selection. */
  private reportsTo(e: MouseEvent): boolean {
    return this.target.isMouseTracking() && !e.shiftKey;
  }

  private report(e: MouseEvent, action: "press" | "release" | "motion", button: number | null) {
    const rect = this.element.getBoundingClientRect();
    const { width, height } = this.options.cell();
    const bytes = this.target.encodeMouse({
      action,
      button,
      mods: modsOf(e),
      x: e.clientX - rect.left,
      y: e.clientY - rect.top,
      screenWidth: rect.width,
      screenHeight: rect.height,
      cellWidth: width,
      cellHeight: height,
      anyButtonPressed: action === "release" ? false : e.buttons !== 0,
    });
    if (bytes.length) this.options.emit(bytes, "mouse");
  }

  private down(e: PointerEvent) {
    this.options.focus();
    const cell = this.cellAt(e.clientX, e.clientY);
    if (e.button === 0 && (this.options.mac ? e.metaKey : e.ctrlKey)) {
      const link = this.linkAt(cell);
      if (link) {
        e.preventDefault();
        this.options.onLink(link.uri, e);
        return;
      }
    }
    if (this.reportsTo(e)) {
      this.reporting = true;
      this.lastReported = cell;
      this.capture(e.pointerId);
      this.report(e, "press", buttonOf(e.button));
      e.preventDefault();
      return;
    }
    if (e.button !== 0) return;
    e.preventDefault();
    this.capture(e.pointerId);
    if (e.detail >= 3) {
      this.target.selectLine(cell.col, cell.row);
      this.anchor = null;
    } else if (e.detail === 2) {
      this.target.selectWord(cell.col, cell.row);
      this.anchor = null;
    } else if (e.shiftKey && this.anchor) {
      this.dragging = true;
      this.moved = true;
      this.select(cell);
    } else {
      this.anchor = this.toScreen(cell);
      this.rectangle = e.altKey;
      this.dragging = true;
      this.moved = false;
      // a drag starts once the pointer leaves this cell
      this.hovered = cell;
      return;
    }
    this.options.redraw();
  }

  private select(end: Cell) {
    if (!this.anchor) return;
    this.target.setSelection(
      { x: this.anchor.col, y: this.anchor.row },
      { x: end.col, y: end.row + this.target.scrollbar().offset },
      { screen: true, rectangle: this.rectangle },
    );
    this.options.redraw();
  }

  private move(e: PointerEvent) {
    const cell = this.cellAt(e.clientX, e.clientY);
    if (this.reporting || (e.buttons === 0 && this.reportsTo(e))) {
      // motion is reported once per cell; the encoder sends it only in
      // the modes that ask for it
      if (this.lastReported?.col === cell.col && this.lastReported?.row === cell.row) return;
      this.lastReported = cell;
      this.report(e, "motion", this.reporting ? buttonOf(firstButton(e.buttons)) : null);
      return;
    }
    if (!this.dragging) {
      this.hover(cell);
      return;
    }
    this.lastPointer = { x: e.clientX, y: e.clientY };
    this.updateAutoscroll(e.clientY);
    const changed = !this.hovered || this.hovered.col !== cell.col || this.hovered.row !== cell.row;
    this.hovered = cell;
    if (changed) {
      this.moved = true;
      this.select(cell);
    }
  }

  private up(e: PointerEvent) {
    if (this.reporting) {
      this.reporting = false;
      this.report(e, "release", buttonOf(e.button));
      return;
    }
    if (this.dragging && !this.moved && e.detail <= 1) {
      // a click without a drag selects nothing
      this.target.clearSelection();
      this.options.redraw();
    }
    this.stopDrag();
  }

  private stopDrag() {
    this.dragging = false;
    this.reporting = false;
    this.stopAutoscroll();
  }

  /** Scrolls while a drag is past the top or bottom edge, and extends the
   *  selection to the row that comes into view. */
  private updateAutoscroll(y: number) {
    const rect = this.element.getBoundingClientRect();
    const { height } = this.options.cell();
    const past = y < rect.top ? y - rect.top : y > rect.bottom ? y - rect.bottom : 0;
    if (past === 0) {
      this.stopAutoscroll();
      return;
    }
    this.autoscrollBy = Math.sign(past) * autoscrollRows(Math.abs(past), height);
    if (this.autoscroll) return;
    this.autoscroll = window.setInterval(() => {
      this.target.scroll(this.autoscrollBy);
      if (this.lastPointer) this.select(this.cellAt(this.lastPointer.x, this.lastPointer.y));
    }, AUTOSCROLL_MS);
  }

  private stopAutoscroll() {
    if (this.autoscroll) window.clearInterval(this.autoscroll);
    this.autoscroll = 0;
  }

  private wheel(e: WheelEvent) {
    const { height } = this.options.cell();
    const step = wheelRows(this.wheelPending, e, height, this.options.rows().length);
    this.wheelPending = step.pending;
    e.preventDefault();
    if (step.rows === 0) return;
    const count = Math.abs(step.rows);
    if (this.reportsTo(e)) {
      // wheel up and down are buttons 4 and 5, pressed once a row
      for (let i = 0; i < count; i++) this.report(e, "press", step.rows < 0 ? 4 : 5);
      return;
    }
    if (this.target.isAlternateScreen()) {
      // alternate scroll: a full-screen program gets arrow keys
      const key = step.rows < 0 ? "ArrowUp" : "ArrowDown";
      const bytes = this.target.encodeKey(new KeyboardEvent("keydown", { key, code: key }));
      for (let i = 0; i < count; i++) this.options.emit(bytes, "key");
      return;
    }
    this.target.scroll(step.rows);
    this.options.scrolled();
  }

  /** The link under a cell: an OSC 8 link, or a plain URL. */
  private linkAt(cell: Cell): Link | null {
    const rows = this.options.rows();
    if (!rows[cell.row]) return null;
    return (
      hyperlinkAt(rows, (x, y) => this.target.hyperlinkAt(x, y), cell.col, cell.row) ?? urlAt(rows, cell.col, cell.row)
    );
  }

  private hover(cell: Cell | null) {
    if (cell && this.hovered?.col === cell.col && this.hovered?.row === cell.row) return;
    this.hovered = cell;
    window.clearTimeout(this.rest);
    const inside = (link: Link | null) =>
      !!cell && !!link?.spans.some((s) => s.row === cell.row && cell.col >= s.start && cell.col < s.end);
    // the pointer is still on the link it shows
    if (inside(this.link)) return;
    this.setLink(null);
    if (!cell) return;
    this.rest = window.setTimeout(() => this.setLink(this.linkAt(cell)), REST_MS);
  }

  private setLink(link: Link | null) {
    if (!link && !this.link) return;
    this.link = link;
    this.element.classList.toggle("is-over-link", !!link);
    this.options.showLink(link?.spans ?? []);
  }

  dispose() {
    this.stopAutoscroll();
    window.clearTimeout(this.rest);
    this.cleanup.forEach((c) => c());
  }
}

/** The DOM button of the lowest bit set in `buttons`. */
function firstButton(buttons: number): number {
  if (buttons & 1) return 0;
  if (buttons & 4) return 1;
  if (buttons & 2) return 2;
  return -1;
}
