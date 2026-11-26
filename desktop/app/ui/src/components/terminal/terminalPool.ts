// The terminals the window keeps attached: the one shown and the last two
// shown before it. A pooled terminal keeps its view, its stream and the
// element the view draws into, so showing it again only moves that
// element back into the pane. The least recently shown terminal beyond
// three is detached and its view disposed; showing it again reattaches it
// with a snapshot.

import { allowFileDrop, dropInto, pasteInto } from "./clipboardPaste";
import { TerminalStream, type ExitInfo, type Role, type SocketLike, type StreamState } from "./stream";
import { currentTheme } from "../../lib/theme";
import type { TerminalRenderer, TerminalTheme, TerminalView, ViewOptions } from "./view";

/** How many terminals stay attached. */
export const POOL_SIZE = 3;
/** How long the window waits for its size to settle before it tells the
 *  host. */
const RESIZE_DEBOUNCE_MS = 50;

type Grid = { cols: number; rows: number };
type Appearance = Pick<ViewOptions, "theme" | "font" | "optionAsMeta" | "screenReader" | "clipboardWrite"> & {
  scheme: "dark" | "light";
};

/** What the pane draws besides the terminal. */
export interface TerminalState {
  stream: StreamState;
  /** Why the stream is reconnecting or closed, when it says. */
  why?: string;
  role: Role;
  /** The PTY's size. */
  pty: Grid | null;
  /** The grid that fits the pane. */
  fit: Grid | null;
  title: string | null;
  exit: ExitInfo | null;
}

export interface PoolDeps {
  renderer(): Promise<TerminalRenderer>;
  /** The view's colours and font, read when a view is made. */
  appearance(): Appearance;
  ticket(terminal: string): Promise<string>;
  url(terminal: string): Promise<string>;
  open?(url: string): SocketLike;
}

export class PooledTerminal {
  readonly element: HTMLDivElement;
  view!: TerminalView;
  stream!: TerminalStream;
  private listeners = new Set<() => void>();
  private resizeTimer: ReturnType<typeof setTimeout> | undefined;
  private look: Appearance | null = null;
  private current: TerminalState = {
    stream: "connecting",
    role: "observer",
    pty: null,
    fit: null,
    title: null,
    exit: null,
  };

  constructor(readonly terminal: string) {
    this.element = document.createElement("div");
    this.element.className = "terminal-surface";
    this.element.dataset.terminal = terminal;
  }

  /** Makes the view in the element, which must be in the pane, and
   *  attaches at the size it fits. */
  async attach(deps: PoolDeps) {
    const renderer = await deps.renderer();
    const look = deps.appearance();
    this.look = look;
    this.view = renderer.create();
    await this.view.mount(this.element, { cols: 80, rows: 24, scrollback: 10_000, ...look });
    const fit = this.measure();
    this.set({ fit });
    this.view.resize(fit.cols, fit.rows);
    this.stream = new TerminalStream({
      terminal: this.terminal,
      view: this.view,
      cols: fit.cols,
      rows: fit.rows,
      ticket: deps.ticket,
      url: deps.url,
      open: deps.open,
      events: {
        state: (stream, why) => {
          this.set({ stream, why });
          if (stream === "live") this.sendFacts();
        },
        role: (role) => {
          this.set({ role });
          this.layout();
        },
        size: (cols, rows) => {
          this.set({ pty: { cols, rows } });
          this.layout();
        },
        title: (title) => this.set({ title }),
        exit: (exit) => this.set({ exit }),
      },
    });
    // a click in an observer's terminal takes control, as a key does
    const take = () => {
      if (this.current.role === "observer" && !this.current.exit) this.stream.control(true);
    };
    this.element.addEventListener("pointerdown", take);
    // files and images on the clipboard, before the renderer sees the paste
    this.element.addEventListener("paste", (event) => pasteInto(this.view, event), { capture: true });
    // files dropped on it, by their paths
    this.element.addEventListener("dragover", allowFileDrop);
    this.element.addEventListener("drop", (event) => dropInto(this.view, event));
    this.stream.start();
  }

  /** The app's theme changed: the view takes its colours and font, and the
   *  host answers the program's colour queries with them. */
  restyle(look: Appearance) {
    if (!this.view) return;
    this.look = look;
    this.view.setTheme(look.theme);
    this.view.setFont(look.font);
    this.view.setScreenReader(look.screenReader ?? false);
    this.view.setInputOptions({ optionAsMeta: look.optionAsMeta, clipboardWrite: look.clipboardWrite });
    this.sendFacts();
    this.refit();
  }

  private sendFacts() {
    const look = this.look;
    if (!look) return;
    this.stream.facts({
      fg: look.theme.foreground,
      bg: look.theme.background,
      cursor: look.theme.cursor,
      color_scheme: look.scheme,
    });
  }

  /** The pane changed size: fit again, and tell the host once it settles. */
  refit() {
    if (!this.view || !this.element.isConnected) return;
    const fit = this.measure();
    if (fit.cols === this.current.fit?.cols && fit.rows === this.current.fit?.rows) return;
    this.set({ fit });
    this.layout();
    clearTimeout(this.resizeTimer);
    this.resizeTimer = setTimeout(() => this.stream.resize(fit.cols, fit.rows), RESIZE_DEBOUNCE_MS);
  }

  /** The controller draws at the size it fits; an observer at the PTY's. */
  private layout() {
    const { role, fit, pty } = this.current;
    const grid = role === "controller" ? fit : (pty ?? fit);
    if (grid) this.view.resize(grid.cols, grid.rows);
  }

  private measure(): Grid {
    const { cols, rows } = this.view.measure();
    return { cols, rows };
  }

  get state(): TerminalState {
    return this.current;
  }

  subscribe(listener: () => void) {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private set(change: Partial<TerminalState>) {
    this.current = { ...this.current, ...change };
    this.listeners.forEach((l) => l());
  }

  dispose() {
    clearTimeout(this.resizeTimer);
    this.stream?.close();
    this.view?.dispose();
    this.element.remove();
    this.listeners.clear();
  }
}

export class TerminalPool {
  /** In the order they were last shown, the most recent last. */
  private entries = new Map<string, Promise<PooledTerminal>>();
  private shown = new Set<string>();

  constructor(
    private readonly deps: PoolDeps,
    private readonly limit = POOL_SIZE,
  ) {}

  /** Puts a terminal in `pane`, attaching it unless it is pooled. The
   *  pane must have its size, so the snapshot comes at the right one. */
  async show(terminal: string, pane: HTMLElement): Promise<PooledTerminal> {
    this.shown.add(terminal);
    let entry = this.entries.get(terminal);
    if (entry) {
      this.entries.delete(terminal);
      this.entries.set(terminal, entry);
      const pooled = await entry;
      pane.append(pooled.element);
      pooled.refit();
      return pooled;
    }
    const pooled = new PooledTerminal(terminal);
    pane.append(pooled.element);
    entry = pooled.attach(this.deps).then(() => pooled);
    this.entries.set(terminal, entry);
    entry.catch(() => this.entries.delete(terminal));
    this.evict();
    return entry;
  }

  /** Takes a terminal out of its pane; it stays attached while pooled. */
  hide(terminal: string) {
    this.shown.delete(terminal);
    void this.entries.get(terminal)?.then((pooled) => {
      if (!this.shown.has(terminal)) pooled.element.remove();
    });
  }

  /** Whether a terminal is attached in the pool. */
  has(terminal: string) {
    return this.entries.has(terminal);
  }

  /** Gives every attached terminal the app's current colours and font. */
  restyle() {
    const look = this.deps.appearance();
    for (const entry of this.entries.values()) void entry.then((p) => p.restyle(look));
  }

  /** The grid a terminal shown in an area of `width` × `height` pixels
   *  will fit: from the renderer's cell in the font, or measured by the
   *  renderer itself in a hidden view. */
  async gridFor(width: number, height: number): Promise<Grid> {
    const renderer = await this.deps.renderer();
    const cell = renderer.cell?.(this.deps.appearance().font);
    if (cell) {
      return { cols: Math.max(1, Math.floor(width / cell.width)), rows: Math.max(1, Math.floor(height / cell.height)) };
    }
    const element = document.createElement("div");
    element.style.cssText = `position: fixed; left: -10000px; top: 0; width: ${width}px; height: ${height}px; visibility: hidden`;
    document.body.append(element);
    const view = renderer.create();
    try {
      await view.mount(element, { cols: 80, rows: 24, scrollback: 0, ...this.deps.appearance() });
      const { cols, rows } = view.measure();
      return { cols, rows };
    } finally {
      view.dispose();
      element.remove();
    }
  }

  /** Detaches every terminal. */
  clear() {
    for (const entry of this.entries.values()) void entry.then((p) => p.dispose());
    this.entries.clear();
    this.shown.clear();
  }

  private evict() {
    for (const [terminal, entry] of this.entries) {
      if (this.entries.size <= this.limit) return;
      if (this.shown.has(terminal)) continue;
      this.entries.delete(terminal);
      void entry.then((p) => p.dispose());
    }
  }
}

/** The view's colours from the app's theme, and its font. */
export function appearanceFrom(element: Element): Appearance {
  const style = getComputedStyle(element);
  const token = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  const theme: TerminalTheme = {
    foreground: token("--text", "#ededef"),
    background: token("--bg", "#18191b"),
    cursor: token("--text", "#ededef"),
    selectionBackground: token("--accent-bg", "#292a3c"),
  };
  return {
    theme,
    font: { family: token("--mono", "monospace"), size: 13, lineHeight: 1.2 },
    scheme: currentTheme(),
  };
}
