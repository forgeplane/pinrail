// xterm.js 6 behind TerminalView: the fallback renderer, drawn with WebGL,
// or with xterm.js's DOM renderer when WebGL cannot start or loses its
// context. It reads only the `vt` snapshot.
//
// xterm.js reports the person's keys, paste, mouse and focus and its own
// replies to queries in the output through one event. The adapter tells
// them apart by where they came from: data emitted while one of the
// person's events is handled is that kind of input, and anything else
// comes from parsing written output, so it is a reply.
//
// Programs may copy to the clipboard with OSC 52, up to 1 MiB, when the
// view allows it; they can never read the clipboard.

import "@xterm/xterm/css/xterm.css";
import { Terminal, type ITheme } from "@xterm/xterm";
import { ClipboardAddon, type ClipboardSelectionType } from "@xterm/addon-clipboard";
import { SearchAddon } from "@xterm/addon-search";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
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

const encoder = new TextEncoder();
/** How long after compositionend xterm.js may send the committed text. */
const COMMIT_WAIT_MS = 500;
/** The most a program may copy with OSC 52. */
const CLIPBOARD_MAX = 1024 * 1024;

const ANSI = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "brightBlack",
  "brightRed",
  "brightGreen",
  "brightYellow",
  "brightBlue",
  "brightMagenta",
  "brightCyan",
  "brightWhite",
] as const;

function toXtermTheme(theme: TerminalTheme): ITheme {
  const out: ITheme = {
    foreground: theme.foreground,
    background: theme.background,
    cursor: theme.cursor,
    selectionBackground: theme.selectionBackground,
  };
  theme.ansi?.forEach((colour, i) => {
    (out as Record<string, string>)[ANSI[i]] = colour;
  });
  return out;
}

/** The person's events and the kind of input xterm.js emits for each. */
const ORIGINS: [string, InputKind][] = [
  ["keydown", "key"],
  ["keypress", "key"],
  ["beforeinput", "key"],
  ["input", "key"],
  ["compositionend", "key"],
  ["paste", "paste"],
  ["mousedown", "mouse"],
  ["mouseup", "mouse"],
  ["mousemove", "mouse"],
  ["wheel", "mouse"],
  ["focusin", "focus"],
  ["focusout", "focus"],
];

class XtermView implements TerminalView, BenchHooks {
  readonly snapshotFormats = ["vt"] as const;
  private term: Terminal | null = null;
  private host: HTMLElement | null = null;
  private search_: SearchAddon | null = null;
  /** The kind of the person's event being handled, if one is. */
  private origin: InputKind | null = null;
  /** The text an input method last committed, which xterm.js sends some
   *  time after compositionend, and until when it is expected. */
  private committed: { text: string; until: number } | null = null;
  private inputs = new Set<(bytes: Uint8Array, kind: InputKind) => void>();
  private replies = new Set<(bytes: Uint8Array) => void>();
  private links = new Set<(uri: string, event: MouseEvent) => void>();
  private cleanup: (() => void)[] = [];
  private webgl = false;

  async mount(host: HTMLElement, options: ViewOptions): Promise<void> {
    this.host = host;
    const term = new Terminal({
      cols: options.cols,
      rows: options.rows,
      scrollback: options.scrollback,
      fontFamily: options.font.family,
      fontSize: options.font.size,
      lineHeight: options.font.lineHeight ?? 1,
      theme: toXtermTheme(options.theme),
      allowProposedApi: true,
      cursorBlink: false,
      // xterm.js has no setting for one Option key alone
      macOptionIsMeta: (options.optionAsMeta ?? "off") !== "off",
      screenReaderMode: options.screenReader ?? false,
    });
    this.term = term;
    term.loadAddon(new Unicode11Addon());
    term.unicode.activeVersion = "11";
    const allowWrite = options.clipboardWrite ?? true;
    term.loadAddon(
      new ClipboardAddon(undefined, {
        // never read: a program cannot learn what the person copied
        readText: () => "",
        writeText: (_selection: ClipboardSelectionType, text: string) => {
          if (allowWrite && text.length <= CLIPBOARD_MAX) void navigator.clipboard?.writeText(text);
        },
      }),
    );
    this.freshSearch();
    term.loadAddon(
      new WebLinksAddon((event, uri) => {
        // links open with Cmd-click on macOS, Ctrl-click elsewhere
        if (event.metaKey || event.ctrlKey) this.links.forEach((l) => l(uri, event));
      }),
    );
    term.options.linkHandler = {
      activate: (event, uri) => {
        if (event.metaKey || event.ctrlKey) this.links.forEach((l) => l(uri, event));
      },
    };
    term.open(host);
    this.useWebgl(term);

    for (const [type, kind] of ORIGINS) {
      const mark = () => {
        this.origin = kind;
        const clear = () => {
          if (this.origin === kind) this.origin = null;
        };
        // xterm.js handles most events at once; after that, what it emits
        // is a reply
        setTimeout(clear);
      };
      host.addEventListener(type, mark, true);
      this.cleanup.push(() => host.removeEventListener(type, mark, true));
    }
    const commit = (event: Event) => {
      const text = (event as CompositionEvent).data;
      this.committed = text ? { text, until: performance.now() + COMMIT_WAIT_MS } : null;
    };
    host.addEventListener("compositionend", commit, true);
    this.cleanup.push(() => host.removeEventListener("compositionend", commit, true));
    const data = term.onData((text) => {
      // a reply is an escape sequence, never the text the person composed
      const committed = this.committed;
      if (committed && text === committed.text && performance.now() < committed.until) {
        this.committed = null;
        this.inputs.forEach((l) => l(encoder.encode(text), "key"));
        return;
      }
      this.route(encoder.encode(text));
    });
    const binary = term.onBinary((text) => {
      const bytes = new Uint8Array(text.length);
      for (let i = 0; i < text.length; i++) bytes[i] = text.charCodeAt(i) & 0xff;
      this.route(bytes);
    });
    this.cleanup.push(
      () => data.dispose(),
      () => binary.dispose(),
    );
  }

  /** A new search addon: it keeps the lines it searched, which a reset
   *  would otherwise leave it finding. */
  private freshSearch() {
    this.search_?.dispose();
    this.search_ = new SearchAddon();
    this.term!.loadAddon(this.search_);
  }

  /** WebGL when it starts, the DOM renderer otherwise and after a lost
   *  context. */
  private useWebgl(term: Terminal) {
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => {
        webgl.dispose();
        this.webgl = false;
      });
      term.loadAddon(webgl);
      this.webgl = true;
    } catch {
      // the DOM renderer stays
    }
  }

  private route(bytes: Uint8Array) {
    const kind = this.origin;
    if (kind) this.inputs.forEach((l) => l(bytes, kind));
    else this.replies.forEach((l) => l(bytes));
  }

  write(bytes: Uint8Array, consumed: () => void): void {
    this.term!.write(bytes, consumed);
  }

  restore(format: string, bytes: Uint8Array): Promise<void> {
    if (format !== "vt") return Promise.reject(new Error(`xterm.js cannot read the ${format} snapshot`));
    this.reset();
    return new Promise((resolve) => this.term!.write(bytes, resolve));
  }

  reset(): void {
    this.term!.reset();
    this.freshSearch();
  }

  measure() {
    // the cell xterm.js draws with, which its fit addon reads the same way
    const core = (
      this.term as unknown as {
        _core: { _renderService: { dimensions: { css: { cell: { width: number; height: number } } } } };
      }
    )._core;
    const cell = core._renderService.dimensions.css.cell;
    const rect = this.host!.getBoundingClientRect();
    return {
      cols: Math.max(1, Math.floor(rect.width / cell.width)),
      rows: Math.max(1, Math.floor(rect.height / cell.height)),
      cellPx: [cell.width, cell.height] as [number, number],
    };
  }

  resize(cols: number, rows: number): void {
    this.term!.resize(cols, rows);
  }

  focus(): void {
    this.term!.focus();
  }

  blur(): void {
    this.term!.blur();
  }

  setTheme(theme: TerminalTheme): void {
    this.term!.options.theme = toXtermTheme(theme);
  }

  setFont(font: TerminalFont): void {
    this.term!.options.fontFamily = font.family;
    this.term!.options.fontSize = font.size;
    if (font.lineHeight) this.term!.options.lineHeight = font.lineHeight;
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
    const subscription = this.term!.onSelectionChange(() => listener(this.selection()));
    return () => subscription.dispose();
  }

  onTitle(listener: (title: string) => void): Unsubscribe {
    const subscription = this.term!.onTitleChange(listener);
    return () => subscription.dispose();
  }

  onLink(listener: (uri: string, event: MouseEvent) => void): Unsubscribe {
    return this.listen(this.links, listener);
  }

  selection(): string | null {
    return this.term!.hasSelection() ? this.term!.getSelection() : null;
  }

  search(query: string, options: SearchOptions): SearchResult {
    const search = this.search_!;
    const found = options.backwards
      ? search.findPrevious(query, { caseSensitive: options.caseSensitive, regex: options.regex })
      : search.findNext(query, { caseSensitive: options.caseSensitive, regex: options.regex });
    return { found };
  }

  scrollToBottom(): void {
    this.term!.scrollToBottom();
  }

  setScreenReader(on: boolean): void {
    this.term!.options.screenReaderMode = on;
  }

  onFrameDrawn(listener: () => void): Unsubscribe {
    const subscription = this.term!.onRender(() => listener());
    return () => subscription.dispose();
  }

  inputElement(): HTMLElement | null {
    return this.term?.textarea ?? null;
  }

  backend(): string {
    return this.webgl ? "webgl2" : "dom";
  }

  dispose(): void {
    this.cleanup.forEach((c) => c());
    this.cleanup = [];
    this.inputs.clear();
    this.replies.clear();
    this.links.clear();
    this.term?.dispose();
    this.term = null;
    this.host = null;
  }
}

export const xtermRenderer: TerminalRenderer = {
  id: "xterm",
  // the code and the stylesheet come with this module
  load: async () => {},
  create: () => new XtermView(),
};
