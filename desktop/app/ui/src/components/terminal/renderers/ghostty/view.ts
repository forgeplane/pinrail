// libghostty-vt behind TerminalView: the renderer chosen for Pinrail. Each
// terminal has its own instance of the library, with its own memory, which
// goes with the view when it is disposed. Output is parsed a piece at a
// time by the write queue and drawn on a canvas; the keyboard, the pointer,
// find and the screen reader mirror each have a module of their own. It
// restores the host's binary snapshot exactly, and a `vt` paint otherwise.

import "./ghostty.css";
import { isMac } from "../../../../lib/keys";
import type {
  BenchHooks,
  InputKind,
  SearchOptions,
  SearchResult,
  TerminalFont,
  TerminalRenderer,
  TerminalTheme,
  TerminalView,
  Unsubscribe,
  ViewOptions,
} from "../../view";
import { GhosttyCore } from "./core";
import { Finder } from "./find";
import { Keyboard } from "./keyboard";
import { Mirror, MirrorElements } from "./mirror";
import { Pointer } from "./pointer";
import { ECHO_MS, WriteQueue } from "./queue";
import { CanvasRenderer, onNextFrame, type Font } from "./renderer";
import { compileGhostty, GhosttyRuntime } from "./runtime";
import type { GhosttyColor, GhosttySnapshot, GhosttyTheme } from "./types";

/** How often streaming output is drawn at most: about 30 times a second.
 *  The echo of a key is drawn at once. */
export const OUTPUT_INTERVAL_MS = 33;

/** How long a hidden view's output must be quiet before the view draws
 *  it, so that it is current when shown. */
export const HIDDEN_QUIET_MS = 100;

/** The binary snapshot of the libghostty-vt build the window loads. */
export const BINARY_FORMAT = "ghostty-bin;vt-1.0.0";
const decoder = new TextDecoder();

/** A CSS colour as RGB: `#rgb`, `#rrggbb`, `#rrggbbaa` or `rgb()`. */
export function parseColour(css: string, fallback: GhosttyColor): GhosttyColor {
  const value = css.trim();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(value)?.[1];
  if (hex) {
    const full = hex.length === 3 ? [...hex].map((c) => c + c).join("") : hex.slice(0, 6);
    const n = parseInt(full, 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
  }
  const rgb = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i.exec(value);
  if (rgb) return { r: +rgb[1], g: +rgb[2], b: +rgb[3] };
  return fallback;
}

const WHITE = { r: 237, g: 237, b: 239 };
const BLACK = { r: 24, g: 25, b: 27 };

function themeOf(theme: TerminalTheme): GhosttyTheme {
  const foreground = parseColour(theme.foreground, WHITE);
  return {
    foreground,
    background: parseColour(theme.background, BLACK),
    cursor: parseColour(theme.cursor, foreground),
    palette: theme.ansi?.map((c) => parseColour(c, foreground)),
  };
}

function fontOf(font: TerminalFont): Font {
  return { family: font.family, size: font.size, lineHeight: font.lineHeight ?? 1 };
}

class GhosttyView implements TerminalView, BenchHooks {
  readonly snapshotFormats = [BINARY_FORMAT, "vt"] as const;
  private host: HTMLElement | null = null;
  private element!: HTMLDivElement;
  private core!: GhosttyCore;
  private renderer!: CanvasRenderer;
  private queue!: WriteQueue;
  private keyboard!: Keyboard;
  private pointer!: Pointer;
  private finder!: Finder;
  private mirror: { mirror: Mirror; elements: MirrorElements } | null = null;
  private snapshot: GhosttySnapshot | null = null;
  private focused = false;
  private lastSelection: string | null = null;
  private output = new TextDecoder();
  private inputs = new Set<(bytes: Uint8Array, kind: InputKind) => void>();
  private replies = new Set<(bytes: Uint8Array) => void>();
  private selections = new Set<(text: string | null) => void>();
  private titles = new Set<(title: string) => void>();
  private links = new Set<(uri: string, event: MouseEvent) => void>();
  private drawnListeners = new Set<() => void>();
  private disposed = false;
  /** Whether output changed the screen while the view was out of sight,
   *  and the timer that draws it once that output is quiet. */
  private missed = false;
  private quiet: ReturnType<typeof setTimeout> | undefined;
  /** The time spent drawing, for the benchmark. */
  private drawing = 0;
  /** When the screen was last drawn, when a key was last typed, and
   *  whether a draw of output waits for its turn. */
  private drawnAt = -Infinity;
  private typedAt = -Infinity;
  private outputWaits = false;

  async mount(host: HTMLElement, options: ViewOptions): Promise<void> {
    const runtime = await GhosttyRuntime.instantiate();
    this.host = host;
    const element = document.createElement("div");
    element.className = "ghostty-terminal";
    element.setAttribute("role", "document");
    element.setAttribute("aria-label", "Terminal");
    host.append(element);
    this.element = element;
    const theme = themeOf(options.theme);
    this.renderer = new CanvasRenderer(
      element,
      fontOf(options.font),
      parseColour(options.theme.selectionBackground ?? "", { r: 60, g: 62, b: 90 }),
    );
    // drawn at once, so that the screen is current in the frame that
    // shows it again
    this.renderer.onShown(() => {
      if (this.missed) this.draw();
    });
    // the symbols font arrived for an icon on screen
    this.renderer.onRedraw(() => this.redraw());
    const { width, height } = this.renderer.cell;
    this.core = GhosttyCore.create(runtime, options.cols, options.rows, width, height, theme, (bytes) =>
      this.replies.forEach((l) => l(bytes)),
    );
    this.queue = this.newQueue();
    // a program may copy to the clipboard (OSC 52), unless the view says
    // not; it can never read it
    const clipboardWrite = options.clipboardWrite ?? true;
    this.core.onClipboardWrite((text) => {
      if (!clipboardWrite) return false;
      void navigator.clipboard?.writeText(text);
      return true;
    });
    this.keyboard = new Keyboard(element, this.core, (bytes, kind) => this.input(bytes, kind), {
      mac: isMac,
      optionAsMeta: options.optionAsMeta ?? "off",
      newline: null,
      copy: (text) => void navigator.clipboard?.writeText(text),
    });
    this.keyboard.input.addEventListener("focus", () => this.setFocused(true));
    this.keyboard.input.addEventListener("blur", () => this.setFocused(false));
    this.finder = new Finder(this.core);
    this.pointer = new Pointer(this.renderer.canvas, this.core, {
      mac: isMac,
      cell: () => this.renderer.cell,
      rows: () => this.snapshot?.rowData ?? [],
      emit: (bytes, kind) => this.input(bytes, kind),
      redraw: () => {
        this.redraw();
        this.selectionChanged();
      },
      showLink: (spans) => {
        this.renderer.setLink(spans);
        this.schedule();
      },
      onLink: (uri, event) => this.links.forEach((l) => l(uri, event)),
      focus: () => this.focus(),
    });
    if (options.screenReader) this.setScreenReader(true);
    this.draw();
  }

  private newQueue() {
    return new WriteQueue(this.core, {
      draw: () => this.drawOutput(),
      title: (title) => this.titles.forEach((l) => l(title)),
    });
  }

  private input(bytes: Uint8Array, kind: InputKind) {
    if (kind === "key") {
      // its echo is drawn without waiting for a frame, and not announced
      this.typedAt = performance.now();
      this.queue.typed();
      this.mirror?.mirror.typed(decoder.decode(bytes));
    }
    this.inputs.forEach((l) => l(bytes, kind));
  }

  private setFocused(focused: boolean) {
    if (this.focused === focused) return;
    this.focused = focused;
    this.schedule();
  }

  /** Draws on the next frame, with the window's other terminals. */
  private schedule() {
    onNextFrame(() => this.draw());
  }

  /** Draws every row on the next frame. */
  private redraw() {
    this.renderer.invalidate();
    this.schedule();
  }

  /** Draws parsed output: at once for the echo of a key or after a
   *  pause, in the frame that parsed it; otherwise at most every 33 ms,
   *  so that a streaming terminal does not draw every frame. */
  private drawOutput() {
    const now = performance.now();
    if (now - this.typedAt < ECHO_MS) {
      this.draw();
      return;
    }
    // a draw already waits for its frame, and draws this output too
    if (this.outputWaits) return;
    const wait = this.drawnAt + OUTPUT_INTERVAL_MS - now;
    if (wait <= 0) {
      this.draw();
      return;
    }
    this.outputWaits = true;
    // the canvas drawn now is painted in the next frame
    setTimeout(() => {
      this.outputWaits = false;
      this.draw();
    }, wait);
  }

  /** Draws the screen, unless the view is out of sight: a pooled view
   *  keeps parsing its output, and draws it once that output has been
   *  quiet for a while, or once it is shown, whichever comes first, so
   *  that showing a terminal that has gone quiet needs no draw. */
  private draw(whileHidden = false) {
    if (this.disposed) return;
    if (!whileHidden && !this.renderer.showing) {
      this.missed = true;
      clearTimeout(this.quiet);
      this.quiet = setTimeout(() => {
        if (this.missed && !this.renderer.showing) this.draw(true);
      }, HIDDEN_QUIET_MS);
      return;
    }
    // the canvas kept what it showed, and the rows that changed since are
    // drawn as after any other output
    this.missed = false;
    const started = performance.now();
    const snapshot = this.core.snapshot();
    this.snapshot = snapshot;
    this.renderer.setMatches(this.finder.matchesInView());
    // output that changed nothing shown, such as a spinning title
    if (!this.renderer.changes(snapshot, this.focused)) {
      this.drawing += performance.now() - started;
      return;
    }
    this.renderer.draw(snapshot, this.focused);
    const { width, height } = this.renderer.cell;
    this.keyboard.moveTo(snapshot.cursorX * width, snapshot.cursorY * height, height);
    this.mirror?.mirror.update(snapshot.rowData);
    this.drawnAt = performance.now();
    this.drawing += this.drawnAt - started;
    this.drawnListeners.forEach((l) => l());
  }

  /** Tells listeners about a selection that changed. */
  private selectionChanged() {
    const text = this.core.selectionText();
    if (text === this.lastSelection) return;
    this.lastSelection = text;
    this.selections.forEach((l) => l(text));
  }

  write(bytes: Uint8Array, consumed: () => void): void {
    if (this.disposed) return;
    if (this.mirror) this.mirror.mirror.output(this.output.decode(bytes, { stream: true }));
    this.queue.write(bytes, consumed);
  }

  async restore(format: string, bytes: Uint8Array): Promise<void> {
    const { width, height } = this.renderer.cell;
    const { cols, rows } = this.core.size();
    if (format === BINARY_FORMAT) this.core.restoreSnapshot(bytes, cols, rows, width, height);
    else if (format === "vt") this.core.resetAndWrite(bytes);
    else throw new Error(`the libghostty renderer cannot read the ${format} snapshot`);
    this.afterReplace();
  }

  reset(): void {
    this.core.resetAndWrite(new Uint8Array());
    this.afterReplace();
  }

  /** The screen was replaced: what waited to be parsed belongs to the old
   *  one, find starts over, and everything is drawn again. */
  private afterReplace() {
    this.queue.stop();
    this.queue = this.newQueue();
    this.finder.clear();
    this.lastSelection = null;
    this.renderer.invalidate();
    this.draw();
    const title = this.core.title();
    if (title) this.titles.forEach((l) => l(title));
  }

  measure() {
    const { width, height } = this.renderer.cell;
    const rect = this.host!.getBoundingClientRect();
    return {
      cols: Math.max(1, Math.floor(rect.width / width)),
      rows: Math.max(1, Math.floor(rect.height / height)),
      cellPx: [width, height] as [number, number],
    };
  }

  resize(cols: number, rows: number): void {
    const { width, height } = this.renderer.cell;
    this.core.resize(cols, rows, width, height);
    this.redraw();
  }

  focus(): void {
    this.keyboard.focus();
    this.setFocused(true);
  }

  blur(): void {
    this.keyboard.blur();
    this.setFocused(false);
  }

  setTheme(theme: TerminalTheme): void {
    this.core.setTheme(themeOf(theme));
    if (theme.selectionBackground)
      this.renderer.setSelectionColour(parseColour(theme.selectionBackground, { r: 60, g: 62, b: 90 }));
    this.redraw();
  }

  setFont(font: TerminalFont): void {
    this.renderer.setFont(fontOf(font));
    const { cols, rows } = this.core.size();
    this.resize(cols, rows);
  }

  private listen<T>(set: Set<T>, listener: T): Unsubscribe {
    set.add(listener);
    return () => set.delete(listener);
  }

  onInput(listener: (bytes: Uint8Array, kind: InputKind) => void): Unsubscribe {
    return this.listen(this.inputs, listener);
  }

  onReply(listener: (bytes: Uint8Array) => void): Unsubscribe {
    return this.listen(this.replies, listener);
  }

  onSelectionChange(listener: (text: string | null) => void): Unsubscribe {
    return this.listen(this.selections, listener);
  }

  onTitle(listener: (title: string) => void): Unsubscribe {
    return this.listen(this.titles, listener);
  }

  onLink(listener: (uri: string, event: MouseEvent) => void): Unsubscribe {
    return this.listen(this.links, listener);
  }

  selection(): string | null {
    return this.core.selectionText();
  }

  search(query: string, options: SearchOptions): SearchResult {
    const result = this.finder.find(query, options);
    this.redraw();
    this.selectionChanged();
    return result;
  }

  scrollToBottom(): void {
    this.core.scrollToBottom();
    this.redraw();
  }

  setScreenReader(on: boolean): void {
    if (on === !!this.mirror) return;
    if (on) {
      const elements = new MirrorElements(this.element);
      this.mirror = { mirror: new Mirror(elements), elements };
      if (this.snapshot) this.mirror.mirror.update(this.snapshot.rowData);
    } else {
      this.mirror!.mirror.dispose();
      this.mirror!.elements.dispose();
      this.mirror = null;
    }
  }

  onFrameDrawn(listener: () => void): Unsubscribe {
    return this.listen(this.drawnListeners, listener);
  }

  inputElement(): HTMLElement | null {
    return this.keyboard?.input ?? null;
  }

  wasmMemoryBytes(): number {
    return this.core?.runtime.memoryBytes().byteLength ?? 0;
  }

  drawMs(): number {
    return this.drawing;
  }

  backend(): string {
    return "canvas2d";
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    clearTimeout(this.quiet);
    this.queue?.stop();
    this.pointer?.dispose();
    this.keyboard?.dispose();
    if (this.mirror) this.setScreenReader(false);
    this.renderer?.dispose();
    this.core?.dispose();
    this.element?.remove();
    for (const set of [this.inputs, this.replies, this.selections, this.titles, this.links, this.drawnListeners])
      set.clear();
    this.host = null;
    this.snapshot = null;
  }
}

export const ghosttyRenderer: TerminalRenderer = {
  id: "ghostty",
  // compiles the library once for the window; rejects where WebAssembly
  // or the file cannot load
  load: async () => {
    await compileGhostty();
  },
  create: () => new GhosttyView(),
};
