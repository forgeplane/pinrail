// A small renderer that keeps the contract's own tests honest before any
// real renderer is registered: plain text in a <pre>, the two input modes
// the contract checks, and an answer to the cursor position query. It is
// for tests only and is never registered.

import type {
  InputKind,
  SearchOptions,
  SearchResult,
  TerminalFont,
  TerminalRenderer,
  TerminalTheme,
  TerminalView,
  Unsubscribe,
  ViewOptions,
} from "../view";

const CELL: [number, number] = [8, 16];
const encoder = new TextEncoder();

class ReferenceView implements TerminalView {
  readonly snapshotFormats = ["vt"] as const;
  private host: HTMLElement | null = null;
  private screen: HTMLPreElement | null = null;
  private keys: HTMLTextAreaElement | null = null;
  private content = "";
  private appCursor = false;
  private bracketed = false;
  private inputs = new Set<(bytes: Uint8Array, kind: InputKind) => void>();
  private replies = new Set<(bytes: Uint8Array) => void>();
  private selections = new Set<(text: string | null) => void>();
  private titles = new Set<(title: string) => void>();
  private cleanup: (() => void)[] = [];

  async mount(host: HTMLElement, _options: ViewOptions) {
    this.host = host;
    this.screen = document.createElement("pre");
    this.screen.style.cssText = "margin: 0; font: 13px/16px monospace; user-select: text";
    this.keys = document.createElement("textarea");
    this.keys.style.cssText = "position: absolute; opacity: 0; width: 1px; height: 1px";
    host.append(this.screen, this.keys);
    const keydown = (event: KeyboardEvent) => this.key(event);
    const paste = (event: ClipboardEvent) => {
      event.preventDefault();
      const pasted = event.clipboardData?.getData("text/plain") ?? "";
      this.emit(this.bracketed ? `\x1b[200~${pasted}\x1b[201~` : pasted, "paste");
    };
    const selection = () => this.selections.forEach((l) => l(this.selection()));
    this.keys.addEventListener("keydown", keydown);
    this.keys.addEventListener("paste", paste);
    document.addEventListener("selectionchange", selection);
    this.cleanup.push(() => document.removeEventListener("selectionchange", selection));
  }

  private emit(data: string, kind: InputKind) {
    const encoded = encoder.encode(data);
    this.inputs.forEach((l) => l(encoded, kind));
  }

  private key(event: KeyboardEvent) {
    const arrows: Record<string, string> = { ArrowUp: "A", ArrowDown: "B", ArrowRight: "C", ArrowLeft: "D" };
    let data: string | null = null;
    if (event.key in arrows) data = (this.appCursor ? "\x1bO" : "\x1b[") + arrows[event.key];
    else if (event.key === "Enter") data = "\r";
    else if (event.key === "Backspace") data = "\x7f";
    else if (event.key.length === 1) data = event.key;
    if (data === null) return;
    event.preventDefault();
    this.emit(data, "key");
  }

  write(bytes: Uint8Array, consumed: () => void) {
    // eslint-disable-next-line no-control-regex
    const sequence = /\x1b(?:\[([?\d;]*)([A-Za-z])|\](\d+);([^\x07]*)\x07|c)/g;
    const data = new TextDecoder().decode(bytes);
    let plain = "";
    let at = 0;
    for (const match of data.matchAll(sequence)) {
      plain += data.slice(at, match.index);
      at = match.index + match[0].length;
      const [, params, final, osc, title] = match;
      if (match[0] === "\x1bc") {
        this.content = "";
        plain = "";
      } else if (osc === "2" || osc === "0") this.titles.forEach((l) => l(title));
      else if (final === "h" || final === "l") {
        if (params === "?1") this.appCursor = final === "h";
        if (params === "?2004") this.bracketed = final === "h";
      } else if (final === "n" && params === "6") {
        const reply = encoder.encode("\x1b[1;1R");
        queueMicrotask(() => this.replies.forEach((l) => l(reply)));
      } else if (final === "J" && params === "2") {
        this.content = "";
        plain = "";
      }
    }
    this.content += plain + data.slice(at);
    if (this.screen) this.screen.textContent = this.content.replace(/\r/g, "");
    queueMicrotask(consumed);
  }

  async restore(_format: string, bytes: Uint8Array) {
    this.reset();
    this.write(bytes, () => {});
  }

  reset() {
    this.content = "";
    this.appCursor = false;
    this.bracketed = false;
    if (this.screen) this.screen.textContent = "";
  }

  measure() {
    const width = this.host?.clientWidth ?? 0;
    const height = this.host?.clientHeight ?? 0;
    return { cols: Math.floor(width / CELL[0]), rows: Math.floor(height / CELL[1]), cellPx: CELL };
  }

  resize(_cols: number, _rows: number) {}
  focus() {
    this.keys?.focus();
  }
  blur() {
    this.keys?.blur();
  }
  setTheme(_theme: TerminalTheme) {}
  setFont(_font: TerminalFont) {}

  private listen<T>(set: Set<T>, listener: T): Unsubscribe {
    set.add(listener);
    return () => set.delete(listener);
  }
  onInput(listener: (bytes: Uint8Array, kind: InputKind) => void) {
    return this.listen(this.inputs, listener);
  }
  onReply(listener: (bytes: Uint8Array) => void) {
    return this.listen(this.replies, listener);
  }
  onSelectionChange(listener: (text: string | null) => void) {
    return this.listen(this.selections, listener);
  }
  onTitle(listener: (title: string) => void) {
    return this.listen(this.titles, listener);
  }
  onLink(_listener: (uri: string, event: MouseEvent) => void): Unsubscribe {
    return () => {};
  }

  paste(text: string) {
    if (text) this.emit(this.bracketed ? `\x1b[200~${text}\x1b[201~` : text, "paste");
  }

  pressKey(key: KeyboardEventInit) {
    const letter = key.key?.toLowerCase() ?? "";
    if (key.ctrlKey && /^[a-z]$/.test(letter)) this.emit(String.fromCharCode(letter.charCodeAt(0) - 96), "key");
  }

  selection() {
    const selected = document.getSelection();
    if (!selected || !this.screen || !this.screen.contains(selected.anchorNode)) return null;
    const value = selected.toString().trim();
    return value === "" ? null : value;
  }

  search(query: string, options: SearchOptions): SearchResult {
    const found = options.regex
      ? new RegExp(query, options.caseSensitive ? "" : "i").test(this.content)
      : options.caseSensitive === false
        ? this.content.toLowerCase().includes(query.toLowerCase())
        : this.content.includes(query);
    return { found };
  }

  scrollToBottom() {}
  setScreenReader(on: boolean) {
    this.screen?.setAttribute("aria-hidden", on ? "false" : "true");
  }

  dispose() {
    this.cleanup.forEach((c) => c());
    this.cleanup = [];
    this.inputs.clear();
    this.replies.clear();
    this.selections.clear();
    this.titles.clear();
    this.screen?.remove();
    this.keys?.remove();
    this.host = null;
  }
}

export const referenceRenderer: TerminalRenderer = {
  id: "reference",
  load: async () => {},
  create: () => new ReferenceView(),
};
