// The window's terminal, as the rest of the UI sees it: one interface,
// TerminalView, which every renderer implements as an adapter in
// renderers/<id>/, and a factory that picks the renderer. Nothing outside
// an adapter's folder imports a terminal library, so adding or switching a
// renderer touches only its adapter and the registry.

export type Unsubscribe = () => void;

export interface ViewOptions {
  cols: number;
  rows: number;
  /** Lines kept above the screen. */
  scrollback: number;
  theme: TerminalTheme;
  font: TerminalFont;
  /** Whether programs may copy to the clipboard (OSC 52); true when left out. */
  clipboardWrite?: boolean;
  /** On macOS, which Option keys act as Meta; off when left out. */
  optionAsMeta?: "off" | "left" | "right" | "both";
  /** Whether the text is mirrored for screen readers; off when left out. */
  screenReader?: boolean;
}

export interface TerminalTheme {
  foreground: string;
  background: string;
  cursor: string;
  selectionBackground?: string;
  /** The 16 ANSI colours, black to bright white. */
  ansi?: readonly string[];
}

export interface TerminalFont {
  family: string;
  size: number;
  lineHeight?: number;
}

export interface SearchOptions {
  caseSensitive?: boolean;
  regex?: boolean;
  backwards?: boolean;
}

export interface SearchResult {
  found: boolean;
  /** Matches in the screen and scrollback, when the renderer counts them. */
  count?: number;
}

export type InputKind = "key" | "paste" | "mouse" | "focus";

export interface TerminalView {
  /** Snapshot formats this renderer reads, best first: ["ghostty-bin;vt-1.0.0", "vt"]. */
  readonly snapshotFormats: readonly string[];
  mount(host: HTMLElement, options: ViewOptions): Promise<void>;
  /** Live output. Calls `consumed` once the bytes are parsed, for credit. */
  write(bytes: Uint8Array, consumed: () => void): void;
  /** Replaces the screen; `format` is one of `snapshotFormats`. */
  restore(format: string, bytes: Uint8Array): Promise<void>;
  /** Clears everything before a resync. */
  reset(): void;
  /** The grid that fits the host element, measured from the font. */
  measure(): { cols: number; rows: number; cellPx: [number, number] };
  resize(cols: number, rows: number): void;
  focus(): void;
  blur(): void;
  setTheme(theme: TerminalTheme): void;
  setFont(font: TerminalFont): void;
  /** Encoded input for the PTY: keys, paste, mouse, focus, in the program's modes. */
  onInput(listener: (bytes: Uint8Array, kind: InputKind) => void): Unsubscribe;
  /** Replies the renderer's emulator generates to queries in the output.
   *  They go to the host as reply frames, never as typed input, so they
   *  never take control or count as the person's input. */
  onReply(listener: (bytes: Uint8Array) => void): Unsubscribe;
  onSelectionChange(listener: (text: string | null) => void): Unsubscribe;
  onTitle(listener: (title: string) => void): Unsubscribe;
  onLink(listener: (uri: string, event: MouseEvent) => void): Unsubscribe;
  selection(): string | null;
  search(query: string, options: SearchOptions): SearchResult;
  scrollToBottom(): void;
  /** Turns the accessible text mirror on or off. */
  setScreenReader(on: boolean): void;
  dispose(): void;
}

/** What a renderer's views tell the terminal benchmark
 *  (`desktop/app/ui/bench/terminal`): when they draw, where keys go, and how
 *  they draw. The app itself never calls these. */
export interface BenchHooks {
  /** Called after the view has drawn a frame. */
  onFrameDrawn(listener: () => void): Unsubscribe;
  /** The element that receives keyboard events. */
  inputElement(): HTMLElement | null;
  /** Bytes of WebAssembly memory this view holds, if it uses any. */
  wasmMemoryBytes?(): number;
  /** The main-thread time the view has spent drawing so far, in ms. */
  drawMs?(): number;
  /** How it draws, such as "canvas2d" or "webgl2". */
  backend(): string;
}

export interface TerminalRenderer {
  readonly id: "xterm" | "ghostty" | string;
  /** Loads code or wasm once; rejects when the renderer cannot run here. */
  load(): Promise<void>;
  create(): TerminalView;
}

/** The registered renderers, by id. Each loads its adapter's module only
 *  when it is tried, so a renderer that is not used costs nothing. */
export type Registry = Readonly<Record<string, () => Promise<TerminalRenderer>>>;

/** What `auto` stands for: the renderer chosen for Pinrail, then xterm.js. */
export const AUTO: readonly string[] = ["ghostty", "xterm"];

/** The renderers to try, in order: the development flag first when it is
 *  set, then the setting `/runtime/terminal/renderer` with `auto`
 *  expanded. An empty setting means `auto`. */
export function rendererOrder(setting: readonly string[] | undefined, flag?: string): string[] {
  const wanted = setting && setting.length > 0 ? setting : ["auto"];
  const ids = [...(flag ? [flag] : []), ...wanted.flatMap((id) => (id === "auto" ? AUTO : [id]))];
  return [...new Set(ids)];
}

/** The renderers that failed to load in this run of the app, with why.
 *  They are not tried again until the app restarts. */
const failed = new Map<string, string>();

export function failedRenderers(): ReadonlyMap<string, string> {
  return failed;
}

/** The renderer the last terminal loaded, by id, or null before any did. */
let loaded: string | null = null;

const loadedListeners = new Set<() => void>();

export function loadedRenderer(): string | null {
  return loaded;
}

/** Calls `listener` whenever a terminal loads a renderer. */
export function onRendererLoaded(listener: () => void): Unsubscribe {
  loadedListeners.add(listener);
  return () => loadedListeners.delete(listener);
}

/** Marks a renderer as failed for the rest of the run, such as one that
 *  lost its GPU context after it loaded. */
export function rendererFailed(id: string, reason: string) {
  failed.set(id, reason);
}

/** The first renderer in `order` that is registered, has not failed in
 *  this run, and loads. Rejects when none does. */
export async function loadRenderer(order: readonly string[], registry: Registry): Promise<TerminalRenderer> {
  for (const id of order) {
    const make = registry[id];
    if (!make || failed.has(id)) continue;
    try {
      const renderer = await make();
      await renderer.load();
      loaded = renderer.id;
      loadedListeners.forEach((listener) => listener());
      return renderer;
    } catch (error) {
      rendererFailed(id, error instanceof Error ? error.message : String(error));
    }
  }
  const tried = order.map((id) => `${id}: ${failed.get(id) ?? (registry[id] ? "not tried" : "not registered")}`);
  throw new Error(`no terminal renderer can run here (${tried.join("; ")})`);
}

/** The renderer the window uses: the order from the setting and the
 *  build's `VITE_TERMINAL_RENDERER`, loaded from `registry`. */
export function chooseRenderer(setting: readonly string[] | undefined, registry: Registry): Promise<TerminalRenderer> {
  const flag = (import.meta.env.VITE_TERMINAL_RENDERER as string | undefined) || undefined;
  return loadRenderer(rendererOrder(setting, flag), registry);
}
