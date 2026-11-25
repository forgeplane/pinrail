// coder/ghostty-web behind TerminalView, for the benchmark only: a
// comparison for the window's libghostty renderer, which the app does not
// register. ghostty-web draws with Canvas 2D over Ghostty built with its own
// patch, and offers an xterm.js-shaped API.
//
// What it leaves out, measured as it is:
// - It reports typed input and its emulator's replies through one event,
//   so both arrive here as key input.
// - It reads only `vt` snapshots.
// - It has no screen reader mirror and no find.
// - It loads its WebAssembly from a `data:` URL first, which the app's
//   policy blocks; the fake core serves the file it falls back to.
// - It never fires its onRender event. Its renderer draws from a frame loop
//   on every frame, even with nothing changed, and right after a write that
//   follows a key, so a frame drawn is a call to its renderer's render().

import { Terminal, init } from "ghostty-web";
import type {
  BenchHooks,
  InputKind,
  SearchResult,
  TerminalFont,
  TerminalRenderer,
  TerminalTheme,
  TerminalView,
  Unsubscribe,
  ViewOptions,
} from "../../../src/components/terminal/view";

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

function themeOf(theme: TerminalTheme): Record<string, string | undefined> {
  const out: Record<string, string | undefined> = {
    foreground: theme.foreground,
    background: theme.background,
    cursor: theme.cursor,
    selectionBackground: theme.selectionBackground,
  };
  theme.ansi?.forEach((colour, i) => (out[ANSI[i]] = colour));
  return out;
}

class GhosttyWebView implements TerminalView, BenchHooks {
  readonly snapshotFormats = ["vt"] as const;
  private term: Terminal | null = null;
  private host: HTMLElement | null = null;
  private element: HTMLDivElement | null = null;
  private inputs = new Set<(bytes: Uint8Array, kind: InputKind) => void>();
  private drawn = new Set<() => void>();
  private cleanup: (() => void)[] = [];

  async mount(host: HTMLElement, options: ViewOptions): Promise<void> {
    this.host = host;
    this.element = document.createElement("div");
    this.element.style.cssText = "width: 100%; height: 100%";
    host.append(this.element);
    const term = new Terminal({
      cols: options.cols,
      rows: options.rows,
      scrollback: options.scrollback,
      fontFamily: options.font.family,
      fontSize: options.font.size,
      theme: themeOf(options.theme),
      cursorBlink: false,
    });
    this.term = term;
    term.open(this.element);
    const renderer = term.renderer!;
    const render = renderer.render.bind(renderer);
    renderer.render = (...args: Parameters<typeof render>) => {
      render(...args);
      this.drawn.forEach((l) => l());
    };
    const encoder = new TextEncoder();
    const data = term.onData((text) => {
      const bytes = encoder.encode(text);
      this.inputs.forEach((l) => l(bytes, "key"));
    });
    this.cleanup.push(() => data.dispose());
  }

  write(bytes: Uint8Array, consumed: () => void): void {
    this.term!.write(bytes, consumed);
  }

  async restore(format: string, bytes: Uint8Array): Promise<void> {
    if (format !== "vt") throw new Error(`ghostty-web cannot read the ${format} snapshot`);
    this.reset();
    await new Promise<void>((resolve) => this.term!.write(bytes, resolve));
  }

  reset(): void {
    this.term!.reset();
  }

  measure() {
    const { width, height } = this.term!.renderer!.getMetrics();
    const rect = this.host!.getBoundingClientRect();
    return {
      cols: Math.max(1, Math.floor(rect.width / width)),
      rows: Math.max(1, Math.floor(rect.height / height)),
      cellPx: [width, height] as [number, number],
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
    this.term!.renderer?.setTheme(themeOf(theme));
  }

  setInputOptions(): void {}

  setFont(font: TerminalFont): void {
    this.term!.renderer?.setFontFamily(font.family);
    this.term!.renderer?.setFontSize(font.size);
  }

  onInput(listener: (bytes: Uint8Array, kind: InputKind) => void): Unsubscribe {
    this.inputs.add(listener);
    return () => this.inputs.delete(listener);
  }

  onReply(): Unsubscribe {
    return () => {};
  }

  onSelectionChange(listener: (text: string | null) => void): Unsubscribe {
    const subscription = this.term!.onSelectionChange(() => listener(this.selection()));
    return () => subscription.dispose();
  }

  onTitle(listener: (title: string) => void): Unsubscribe {
    const subscription = this.term!.onTitleChange(listener);
    return () => subscription.dispose();
  }

  onLink(): Unsubscribe {
    return () => {};
  }

  selection(): string | null {
    return this.term!.hasSelection() ? this.term!.getSelection() : null;
  }

  // the benchmark types its keys as events: nothing is pasted or pressed
  paste(): void {}

  pressKey(): void {}

  search(): SearchResult {
    return { found: false };
  }

  scrollToBottom(): void {
    this.term!.scrollToBottom();
  }

  setScreenReader(): void {}

  onFrameDrawn(listener: () => void): Unsubscribe {
    this.drawn.add(listener);
    return () => this.drawn.delete(listener);
  }

  inputElement(): HTMLElement | null {
    return this.term?.textarea ?? null;
  }

  backend(): string {
    return "canvas2d";
  }

  dispose(): void {
    this.cleanup.forEach((c) => c());
    this.cleanup = [];
    this.inputs.clear();
    this.drawn.clear();
    this.term?.dispose();
    this.term = null;
    this.element?.remove();
    this.host = null;
  }
}

export const ghosttyWebRenderer: TerminalRenderer = {
  id: "ghostty-web",
  // loads and compiles its WebAssembly once for the page
  load: () => init(),
  create: () => new GhosttyWebView(),
};
