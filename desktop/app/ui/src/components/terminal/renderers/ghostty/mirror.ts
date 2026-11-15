// The screen reader's view of a libghostty terminal, made only when
// /runtime/terminal/screen_reader is on. A text mirror of the visible rows,
// one element per row off screen, is updated at most every 100 ms and only
// for rows that changed. New output is announced through a polite live
// region, batched over 250 ms and cut to its last 500 characters, without
// the characters the person typed, which the screen reader already echoes.

import type { GhosttyRow } from "./types";

/** How often the rows are brought up to date, at most. */
export const ROWS_MS = 100;
/** How long output is collected before it is announced. */
export const ANNOUNCE_MS = 250;
/** How much of a batch is announced: its end. */
export const ANNOUNCE_LIMIT = 500;
/** How long typed text waits for its echo. */
const ECHO_MS = 1000;

/** Where the mirror puts its text. */
export interface MirrorSink {
  setRowCount(count: number): void;
  setRow(index: number, text: string): void;
  announce(text: string): void;
}

/** Output as text a person reads: escape sequences and control characters
 *  removed, and line breaks as "\n". */
export function readableText(output: string): string {
  /* eslint-disable no-control-regex -- it removes control characters */
  return (
    output
      // OSC, DCS, APC, PM and SOS strings, ended by BEL or ST
      .replace(/\x1b[\]P_^X][\s\S]*?(?:\x07|\x1b\\)/g, "")
      // CSI sequences, then other escapes
      .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "")
      .replace(/\x1b[ -/]*[0-~]/g, "")
      .replace(/\r\n|\r/g, "\n")
      // other control characters, except line breaks and tabs
      .replace(/[\x00-\x08\x0b-\x1f\x7f]/g, "")
  );
  /* eslint-enable no-control-regex */
}

export class Mirror {
  private texts: string[] = [];
  private rows: readonly GhosttyRow[] | null = null;
  private rowTimer: ReturnType<typeof setTimeout> | null = null;
  private lastRows = -Infinity;
  private pending = "";
  private announceTimer: ReturnType<typeof setTimeout> | null = null;
  private echo = "";
  private echoTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(private readonly sink: MirrorSink) {}

  /** Takes the visible rows after a draw; they reach the sink at most
   *  every 100 ms. */
  update(rows: readonly GhosttyRow[]) {
    this.rows = rows;
    if (this.rowTimer) return;
    const wait = Math.max(0, this.lastRows + ROWS_MS - Date.now());
    if (wait === 0) {
      this.flushRows();
      return;
    }
    this.rowTimer = setTimeout(() => {
      this.rowTimer = null;
      this.flushRows();
    }, wait);
  }

  private flushRows() {
    const rows = this.rows;
    if (!rows) return;
    this.rows = null;
    this.lastRows = Date.now();
    if (rows.length !== this.texts.length) {
      this.sink.setRowCount(rows.length);
      this.texts.length = rows.length;
    }
    rows.forEach((row, i) => {
      const text = row.text.trimEnd();
      if (this.texts[i] === text) return;
      this.texts[i] = text;
      this.sink.setRow(i, text);
    });
  }

  /** Text the person typed, whose echo is not announced. */
  typed(text: string) {
    this.echo += text;
    if (this.echoTimer) clearTimeout(this.echoTimer);
    this.echoTimer = setTimeout(() => {
      this.echo = "";
      this.echoTimer = null;
    }, ECHO_MS);
  }

  /** Output from the program, announced in batches. */
  output(text: string) {
    let readable = readableText(text);
    // the echo of what was typed, in the order it was typed
    while (this.echo && readable && readable[0] === this.echo[0]) {
      readable = readable.slice(1);
      this.echo = this.echo.slice(1);
    }
    if (!readable.trim()) return;
    this.pending += readable;
    if (this.announceTimer) return;
    this.announceTimer = setTimeout(() => {
      this.announceTimer = null;
      const text = this.pending.slice(-ANNOUNCE_LIMIT).trim();
      this.pending = "";
      if (text) this.sink.announce(text);
    }, ANNOUNCE_MS);
  }

  dispose() {
    for (const timer of [this.rowTimer, this.announceTimer, this.echoTimer]) if (timer) clearTimeout(timer);
  }
}

/** The mirror's elements: rows off screen, and a polite live region. */
export class MirrorElements implements MirrorSink {
  readonly element: HTMLDivElement;
  private readonly list: HTMLDivElement;
  private readonly live: HTMLDivElement;

  constructor(host: HTMLElement) {
    this.element = document.createElement("div");
    this.element.className = "ghostty-mirror";
    // off screen, yet in the accessibility tree, wherever it is mounted
    this.element.style.cssText =
      "position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%);white-space:pre";
    this.list = document.createElement("div");
    this.list.setAttribute("role", "list");
    this.list.setAttribute("aria-label", "Terminal screen");
    this.live = document.createElement("div");
    this.live.setAttribute("aria-live", "polite");
    this.live.setAttribute("aria-atomic", "true");
    this.element.append(this.list, this.live);
    host.append(this.element);
  }

  setRowCount(count: number) {
    while (this.list.children.length > count) this.list.lastElementChild!.remove();
    while (this.list.children.length < count) {
      const row = document.createElement("div");
      row.setAttribute("role", "listitem");
      this.list.append(row);
    }
  }

  setRow(index: number, text: string) {
    this.list.children[index].textContent = text;
  }

  announce(text: string) {
    this.live.textContent = text;
  }

  dispose() {
    this.element.remove();
  }
}
