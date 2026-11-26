// One terminal on libghostty-vt 1.0: parsing, the render state the
// renderer draws, key, paste and mouse encoding, selection, links, search,
// the title and the binary snapshot.
//
// Adapted from T3 Code (https://github.com/pingdotgg/t3code), MIT licence,
// Copyright (c) 2026 T3 Tools Inc., apps/web/src/terminal/ghostty/core.ts at
// commit ade47301, ported to the libghostty-vt 1.0 C API: option and data
// numbers are looked up by name in ghostty_type_json; ghostty_terminal_new
// takes the size; modes go through GhosttyTerminalModeConfig; cell text is
// read as UTF-8; the selection's text comes from the formatter; structs
// passed by value go by address, as the wasm32 C ABI passes them; and the
// binary snapshot, search and title are new. The licence is in
// licenses/t3code.txt.

import type { GhosttyCell, GhosttyColor, GhosttyRow, GhosttySnapshot, GhosttyTheme } from "./types";
import {
  type GhosttyKeyboardLayoutMap,
  ghosttyConsumedMods,
  ghosttyKeyForEvent,
  ghosttyUnshiftedCodepoint,
  loadGhosttyKeyboardLayoutMap,
} from "./keyCodes";
import type { GhosttyRuntime } from "./runtime";

export type { GhosttyColor, GhosttySnapshot, GhosttyTheme };

const SUCCESS = 0;
const OUT_OF_SPACE = -3;
const NO_VALUE = -4;
/** Lines of history, as many as the host keeps (`SCROLLBACK_LINES` in the host). */
const SCROLLBACK_LINES = 10_000;
/** The most a program may copy to the clipboard at once. */
const CLIPBOARD_MAX_BYTES = 1024 * 1024;
/** DEC private mode 2027, grapheme clustering. */
const GRAPHEME_CLUSTER = 2027;
const SIZE_MAX = 0xffffffff;
const UTF8_INITIAL = 64;

const decoder = new TextDecoder();
/** The one-character strings of ASCII, by code. */
const ASCII = Array.from({ length: 128 }, (_, i) => String.fromCharCode(i));
const encoder = new TextEncoder();

function blend(foreground: GhosttyColor, background: GhosttyColor): GhosttyColor {
  const channel = (front: number, back: number) => Math.floor((front * 155 + back * 100) / 255);
  return {
    r: channel(foreground.r, background.r),
    g: channel(foreground.g, background.g),
    b: channel(foreground.b, background.b),
  };
}

/** Option, data and enum numbers of the loaded build, by name. */
function constants(rt: GhosttyRuntime) {
  const opt = (k: string) => rt.enumValue("GhosttyTerminalOption", k);
  const data = (k: string) => rt.enumValue("GhosttyTerminalData", k);
  const rs = (k: string) => rt.enumValue("GhosttyRenderStateData", k);
  const row = (k: string) => rt.enumValue("GhosttyRenderStateRowData", k);
  const cell = (k: string) => rt.enumValue("GhosttyRenderStateRowCellsData", k);
  return {
    opt: {
      fg: opt("COLOR_FOREGROUND"),
      bg: opt("COLOR_BACKGROUND"),
      cursor: opt("COLOR_CURSOR"),
      palette: opt("COLOR_PALETTE"),
      selection: opt("SELECTION"),
      cursorBlink: opt("DEFAULT_CURSOR_BLINK"),
      mode: opt("MODE"),
      modeDefault: opt("MODE_DEFAULT"),
      scrollbackBytes: opt("SCROLLBACK_MAX_BYTES"),
      scrollbackLines: opt("SCROLLBACK_MAX_LINES"),
    },
    data: {
      palette: data("COLOR_PALETTE"),
      selection: data("SELECTION"),
      mode: data("MODE"),
      title: data("TITLE"),
      pwd: data("PWD"),
      mouseTracking: data("MOUSE_TRACKING"),
      activeScreen: data("ACTIVE_SCREEN"),
      viewportActive: data("VIEWPORT_ACTIVE"),
      scrollbar: data("SCROLLBAR"),
    },
    rs: {
      cols: rs("COLS"),
      rows: rs("ROWS"),
      dirty: rs("DIRTY"),
      rowIterator: rs("ROW_ITERATOR"),
      background: rs("COLOR_BACKGROUND"),
      foreground: rs("COLOR_FOREGROUND"),
      cursor: rs("COLOR_CURSOR"),
      cursorHasValue: rs("COLOR_CURSOR_HAS_VALUE"),
      cursorStyle: rs("CURSOR_VISUAL_STYLE"),
      cursorVisible: rs("CURSOR_VISIBLE"),
      cursorBlinking: rs("CURSOR_BLINKING"),
      cursorInViewport: rs("CURSOR_VIEWPORT_HAS_VALUE"),
      cursorX: rs("CURSOR_VIEWPORT_X"),
      cursorY: rs("CURSOR_VIEWPORT_Y"),
      optDirty: rt.enumValue("GhosttyRenderStateOption", "DIRTY"),
      rowOptDirty: rt.enumValue("GhosttyRenderStateRowOption", "DIRTY"),
      dirtyFull: rt.enumValue("GhosttyRenderStateDirty", "FULL"),
    },
    row: { dirty: row("DIRTY"), raw: row("RAW"), cells: row("CELLS") },
    rawRow: {
      wrap: rt.enumValue("GhosttyRowData", "WRAP"),
      wrapContinuation: rt.enumValue("GhosttyRowData", "WRAP_CONTINUATION"),
    },
    cell: {
      raw: cell("RAW"),
      style: cell("STYLE"),
      selected: cell("SELECTED"),
      utf8: cell("GRAPHEMES_UTF8"),
      bg: cell("BG_COLOR"),
      fg: cell("FG_COLOR"),
    },
    rawCellWide: rt.enumValue("GhosttyCellData", "WIDE"),
    scroll: {
      bottom: rt.enumValue("GhosttyTerminalScrollViewportTag", "BOTTOM"),
      delta: rt.enumValue("GhosttyTerminalScrollViewportTag", "DELTA"),
    },
    point: { viewport: rt.enumValue("GhosttyPointTag", "VIEWPORT"), screen: rt.enumValue("GhosttyPointTag", "SCREEN") },
    formatter: {
      unwrap: rt.enumValue("GhosttyFormatterOption", "UNWRAP"),
      trim: rt.enumValue("GhosttyFormatterOption", "TRIM"),
      selection: rt.enumValue("GhosttyFormatterOption", "SELECTION"),
    },
    search: {
      needle: rt.enumValue("GhosttySearchOption", "NEEDLE"),
      total: rt.enumValue("GhosttySearchData", "TOTAL_MATCHES"),
      selected: rt.enumValue("GhosttySearchData", "SELECTED_MATCH"),
      inView: rt.enumValue("GhosttySearchData", "VIEWPORT_MATCHES"),
    },
    mouse: {
      size: rt.enumValue("GhosttyMouseEncoderOption", "SIZE"),
      anyButton: rt.enumValue("GhosttyMouseEncoderOption", "ANY_BUTTON_PRESSED"),
      trackLastCell: rt.enumValue("GhosttyMouseEncoderOption", "TRACK_LAST_CELL"),
    },
  };
}

type Constants = ReturnType<typeof constants>;

export interface GhosttyMouseInput {
  readonly action: "press" | "release" | "motion";
  /** GhosttyMouseButton: 1 left, 2 right, 3 middle, 4 and 5 wheel; null for none. */
  readonly button: number | null;
  readonly mods: number;
  readonly x: number;
  readonly y: number;
  readonly screenWidth: number;
  readonly screenHeight: number;
  readonly cellWidth: number;
  readonly cellHeight: number;
  readonly anyButtonPressed: boolean;
}

export class GhosttyCore {
  private readonly rt: GhosttyRuntime;
  private readonly k: Constants;
  private terminal = 0;
  private renderState = 0;
  private rowIteratorSlot = 0;
  private rowCellsSlot = 0;
  private keyEncoder = 0;
  private keyEvent = 0;
  private mouseEncoder = 0;
  private mouseEvent = 0;
  private searchHandle = 0;
  private gridCols = 0;
  private gridRows = 0;
  private searchNeedle = "";
  private ptyWriterId = 0;
  private ptyWriter: ((data: Uint8Array) => void) | null = null;
  private bell: (() => void) | null = null;
  private clipboardWrite: ((text: string) => boolean) | null = null;
  /** The palette, for the snapshot being read. */
  private palette: Uint8Array | null = null;
  private theme: GhosttyTheme | null = null;
  // Scratch memory reused across calls: 32 bytes of scratch, the cell query
  // (5 keys, 5 value pointers, written), its outputs, and a UTF-8 buffer.
  private scratch = 0;
  private query = 0;
  private style = 0;
  private utf8Desc = 0;
  private utf8 = 0;
  private utf8Capacity = 0;
  /** The input buffer that writes reuse, grown up to `INPUT_LIMIT`. */
  private input = 0;
  private inputCapacity = 0;
  /** Whether a write is in libghostty-vt, so that one made meanwhile
   *  does not reuse the input buffer. */
  private writing = false;
  /** Room for the structures that scrolling, the scrollbar, the mouse
   *  encoder and points pass in and out. */
  private work = 0;
  private rows: GhosttyRow[] = [];
  private disposed = false;
  private keyboardLayoutMap: GhosttyKeyboardLayoutMap | undefined;

  private constructor(rt: GhosttyRuntime) {
    this.rt = rt;
    this.k = constants(rt);
    void loadGhosttyKeyboardLayoutMap().then((m) => {
      if (!this.disposed) this.keyboardLayoutMap = m;
    });
  }

  /** A terminal on `rt`, an instance of the library for it alone. What
   *  it writes to its PTY, the replies to the program's queries, goes to
   *  `onPtyData`. */
  static create(
    rt: GhosttyRuntime,
    cols: number,
    rows: number,
    cellWidth: number,
    cellHeight: number,
    theme: GhosttyTheme,
    onPtyData: (data: Uint8Array) => void,
  ): GhosttyCore {
    const core = new GhosttyCore(rt);
    try {
      core.initialize(cols, rows, cellWidth, cellHeight, theme, onPtyData);
      return core;
    } catch (error) {
      core.dispose();
      throw error;
    }
  }

  get runtime(): GhosttyRuntime {
    return this.rt;
  }

  private newHandle(fn: string, ...args: number[]): number {
    const slot = this.rt.allocOpaque();
    try {
      this.ok(fn, this.rt.call(fn, 0, slot, ...args));
      return this.rt.readPointer(slot);
    } finally {
      this.rt.freeOpaque(slot);
    }
  }

  private initialize(
    cols: number,
    rows: number,
    cellWidth: number,
    cellHeight: number,
    theme: GhosttyTheme,
    onPtyData: (data: Uint8Array) => void,
  ): void {
    this.scratch = this.rt.alloc(32);
    this.terminal = this.newHandle("ghostty_terminal_new", clamp16(cols), clamp16(rows));
    this.configureTerminal();
    this.ptyWriter = onPtyData;
    this.ptyWriterId = this.rt.attachPtyWriter(this.terminal, onPtyData);

    this.renderState = this.newHandle("ghostty_render_state_new");
    // The row iterator and row cells are filled through these slots.
    this.rowIteratorSlot = this.rt.allocOpaque();
    this.ok("row_iterator_new", this.rt.call("ghostty_render_state_row_iterator_new", 0, this.rowIteratorSlot));
    this.rowCellsSlot = this.rt.allocOpaque();
    this.ok("row_cells_new", this.rt.call("ghostty_render_state_row_cells_new", 0, this.rowCellsSlot));
    this.keyEncoder = this.newHandle("ghostty_key_encoder_new");
    this.keyEvent = this.newHandle("ghostty_key_event_new");
    this.mouseEncoder = this.newHandle("ghostty_mouse_encoder_new");
    this.mouseEvent = this.newHandle("ghostty_mouse_event_new");

    // The cell query: STYLE, SELECTED, GRAPHEMES_UTF8, then the background,
    // last because a colour the cell does not have stops the query. The
    // foreground is read from the style.
    this.style = this.rt.alloc(this.rt.layout("GhosttyStyle").size);
    this.utf8Desc = this.rt.alloc(this.rt.layout("GhosttyBuffer").size);
    this.growUtf8(UTF8_INITIAL);
    this.query = this.rt.alloc(64);
    this.work = this.rt.alloc(WORK_SIZE);
    const q = this.rt.memoryDataView();
    const c = this.k.cell;
    const keys = [c.style, c.selected, c.utf8, c.bg];
    const values = [this.style, this.scratch + 16, this.utf8Desc, this.scratch + 20];
    keys.forEach((key, i) => {
      q.setUint32(this.query + i * 4, key, true);
      q.setUint32(this.query + 20 + i * 4, values[i]!, true);
    });
    this.setTheme(theme);
    this.resize(cols, rows, cellWidth, cellHeight);
  }

  /** Scrollback and cursor defaults, again after a snapshot replaces the terminal. */
  private configureTerminal(restored = false): void {
    this.setSize(this.k.opt.scrollbackBytes, SIZE_MAX);
    this.setSize(this.k.opt.scrollbackLines, SCROLLBACK_LINES);
    this.rt.bytes(this.scratch, 1)[0] = 1;
    this.rt.call("ghostty_terminal_set", this.terminal, this.k.opt.cursorBlink, this.scratch);
    // grapheme clustering (mode 2027), as the host's terminal has it, so
    // that emoji with VS16 and ZWJ sequences take the same width in both:
    // on as the reset default, which also sets it now, and off again for a
    // restored terminal whose program had turned it off
    const wasOn = this.mode(GRAPHEME_CLUSTER);
    this.setMode(this.k.opt.modeDefault, GRAPHEME_CLUSTER, true);
    if (restored && !wasOn) this.setMode(this.k.opt.mode, GRAPHEME_CLUSTER, false);
  }

  /** Sets a DEC private mode now (OPT_MODE) or as the reset default
   *  (OPT_MODE_DEFAULT), through a GhosttyTerminalModeConfig. */
  private setMode(option: number, mode: number, value: boolean): void {
    const v = this.rt.memoryDataView();
    v.setUint16(this.scratch, mode & 0x7fff, true);
    v.setUint8(this.scratch + 2, value ? 1 : 0);
    this.ok("terminal_set", this.rt.call("ghostty_terminal_set", this.terminal, option, this.scratch));
  }

  private setSize(option: number, value: number): void {
    this.rt.memoryDataView().setUint32(this.scratch, value >>> 0, true);
    this.ok("terminal_set", this.rt.call("ghostty_terminal_set", this.terminal, option, this.scratch));
  }

  private growUtf8(size: number): void {
    if (this.utf8) this.rt.free(this.utf8, this.utf8Capacity);
    this.utf8 = this.rt.alloc(size);
    this.utf8Capacity = size;
  }

  write(data: Uint8Array): void {
    this.ensureActive();
    if (data.length === 0) return;
    // a write up to the limit goes through the input buffer; a larger
    // one, such as a replay, through memory of its own, freed after
    const reuse = data.length <= INPUT_LIMIT && !this.writing;
    if (reuse && data.length > this.inputCapacity) {
      if (this.input) this.rt.free(this.input, this.inputCapacity);
      this.inputCapacity = Math.min(INPUT_LIMIT, Math.max(data.length, this.inputCapacity * 2, INPUT_INITIAL));
      this.input = this.rt.alloc(this.inputCapacity);
    }
    const pointer = reuse ? this.input : this.rt.alloc(data.length);
    // after any growth of memory, which detaches the old view
    this.rt.bytes(pointer, data.length).set(data);
    this.writing = true;
    try {
      this.rt.call("ghostty_terminal_vt_write", this.terminal, pointer, data.length);
    } finally {
      this.writing = false;
      if (!reuse) this.rt.free(pointer, data.length);
    }
  }

  /** The work area, cleared for a structure of `size` bytes. */
  private workArea(size: number): number {
    if (size > WORK_SIZE) throw new Error(`libghostty-vt structure of ${size} bytes is larger than the work area`);
    this.rt.memoryBytes().fill(0, this.work, this.work + size);
    return this.work;
  }

  /** Clears the terminal and replays VT without answering the queries in it. */
  resetAndWrite(data: Uint8Array): void {
    this.ensureActive();
    this.rt.call("ghostty_terminal_reset", this.terminal);
    this.configureTerminal();
    if (this.theme) this.setTheme(this.theme);
    this.rows = [];
    if (data.length === 0) return;
    if (this.ptyWriterId) this.rt.detachPtyWriter(this.terminal, this.ptyWriterId);
    this.ptyWriterId = 0;
    try {
      this.write(data);
    } finally {
      this.attachWriters();
    }
  }

  /**
   * Replaces the terminal with one decoded from a binary snapshot of the same
   * libghostty build (ghostty_snapshot_encode on the host). The old terminal
   * is freed first, so that memory never holds both; a snapshot that cannot
   * be decoded leaves a blank terminal, and throws.
   */
  restoreSnapshot(bytes: Uint8Array, cols: number, rows: number, cellWidth: number, cellHeight: number): void {
    this.ensureActive();
    this.freeSearch();
    if (this.ptyWriterId) this.rt.detachPtyWriter(this.terminal, this.ptyWriterId);
    this.ptyWriterId = 0;
    this.rt.call("ghostty_terminal_free", this.terminal);
    this.terminal = 0;
    let next = 0;
    let failure: unknown = null;
    const pointer = this.rt.alloc(bytes.length);
    this.rt.bytes(pointer, bytes.length).set(bytes);
    let decoderHandle = 0;
    try {
      decoderHandle = this.newHandle("ghostty_snapshot_decoder_new_buf", pointer, bytes.length);
      next = this.newHandleFrom("ghostty_snapshot_decoder_decode", decoderHandle);
    } catch (error) {
      failure = error;
    } finally {
      if (decoderHandle) this.rt.call("ghostty_snapshot_decoder_free", decoderHandle);
      this.rt.free(pointer, bytes.length);
    }
    this.terminal = next || this.newHandle("ghostty_terminal_new", clamp16(cols), clamp16(rows));
    this.configureTerminal(!!next);
    if (this.theme) this.setTheme(this.theme);
    this.attachWriters();
    this.rows = [];
    this.resize(cols, rows, cellWidth, cellHeight);
    if (failure) throw failure;
  }

  private newHandleFrom(fn: string, handle: number): number {
    const slot = this.rt.allocOpaque();
    try {
      this.ok(fn, this.rt.call(fn, handle, slot));
      return this.rt.readPointer(slot);
    } finally {
      this.rt.freeOpaque(slot);
    }
  }

  /** The grid's columns and rows. */
  size(): { cols: number; rows: number } {
    return { cols: this.gridCols, rows: this.gridRows };
  }

  resize(cols: number, rows: number, cellWidth: number, cellHeight: number): void {
    this.ensureActive();
    this.gridCols = clamp16(cols);
    this.gridRows = clamp16(rows);
    this.ok(
      "terminal_resize",
      this.rt.call(
        "ghostty_terminal_resize",
        this.terminal,
        clamp16(cols),
        clamp16(rows),
        Math.max(1, Math.round(cellWidth)),
        Math.max(1, Math.round(cellHeight)),
      ),
    );
  }

  setTheme(theme: GhosttyTheme): void {
    this.ensureActive();
    this.theme = theme;
    const color = this.rt.alloc(3);
    for (const [option, value] of [
      [this.k.opt.fg, theme.foreground],
      [this.k.opt.bg, theme.background],
      [this.k.opt.cursor, theme.cursor],
    ] as const) {
      this.rt.bytes(color, 3).set([value.r, value.g, value.b]);
      this.rt.call("ghostty_terminal_set", this.terminal, option, color);
    }
    this.rt.free(color, 3);
    if (theme.palette) {
      const size = 256 * 3;
      const palette = this.rt.alloc(size);
      this.rt.call("ghostty_terminal_get", this.terminal, this.k.data.palette, palette);
      const bytes = this.rt.bytes(palette, size);
      theme.palette.slice(0, 16).forEach((c, i) => bytes.set([c.r, c.g, c.b], i * 3));
      this.rt.call("ghostty_terminal_set", this.terminal, this.k.opt.palette, palette);
      this.rt.free(palette, size);
    }
  }

  private scrollViewport(tag: number, delta = 0): void {
    const p = this.workArea(this.rt.layout("GhosttyTerminalScrollViewport").size);
    const v = this.rt.memoryDataView();
    v.setUint32(p, tag, true);
    v.setInt32(p + this.rt.field("GhosttyTerminalScrollViewport", "value").offset, delta, true);
    this.rt.call("ghostty_terminal_scroll_viewport", this.terminal, p);
  }

  scroll(deltaRows: number): void {
    this.ensureActive();
    this.scrollViewport(this.k.scroll.delta, deltaRows);
  }

  scrollToBottom(): void {
    this.ensureActive();
    this.scrollViewport(this.k.scroll.bottom);
  }

  private getBool(data: number): boolean {
    this.rt.bytes(this.scratch, 1)[0] = 0;
    return (
      this.rt.call("ghostty_terminal_get", this.terminal, data, this.scratch) === SUCCESS &&
      this.rt.bytes(this.scratch, 1)[0] !== 0
    );
  }

  /** A DEC private mode (ansi=false) or an ANSI mode. */
  mode(value: number, ansi = false): boolean {
    this.ensureActive();
    const v = this.rt.memoryDataView();
    v.setUint16(this.scratch, (value & 0x7fff) | (ansi ? 0x8000 : 0), true);
    v.setUint8(this.scratch + 2, 0);
    return (
      this.rt.call("ghostty_terminal_get", this.terminal, this.k.data.mode, this.scratch) === SUCCESS &&
      v.getUint8(this.scratch + 2) !== 0
    );
  }

  isSynchronizedOutput(): boolean {
    return this.mode(2026);
  }

  isMouseTracking(): boolean {
    this.ensureActive();
    return this.getBool(this.k.data.mouseTracking);
  }

  /** Whether the program shows the alternate screen, as full-screen
   *  programs do. */
  isAlternateScreen(): boolean {
    this.ensureActive();
    this.rt.memoryDataView().setUint32(this.scratch, 0, true);
    this.rt.call("ghostty_terminal_get", this.terminal, this.k.data.activeScreen, this.scratch);
    return this.rt.memoryDataView().getUint32(this.scratch, true) === 1;
  }

  /** The screen's rows, scrollback included, and where the viewport's top
   *  row is among them. */
  scrollbar(): { total: number; offset: number; length: number } {
    this.ensureActive();
    const p = this.workArea(24);
    this.rt.call("ghostty_terminal_get", this.terminal, this.k.data.scrollbar, p);
    const v = this.rt.memoryDataView();
    return {
      total: Number(v.getBigUint64(p, true)),
      offset: Number(v.getBigUint64(p + 8, true)),
      length: Number(v.getBigUint64(p + 16, true)),
    };
  }

  isViewportActive(): boolean {
    this.ensureActive();
    return this.getBool(this.k.data.viewportActive);
  }

  /** The window title set by OSC 0 or 2, or "" when none. */
  title(): string {
    return this.text(this.k.data.title);
  }

  /** The working folder the program reported (OSC 7), as its URL. */
  pwd(): string {
    return this.text(this.k.data.pwd);
  }

  /** Has `onBell` called when the program rings the bell. */
  onBell(onBell: () => void): void {
    this.ensureActive();
    this.bell = onBell;
    this.rt.attachBell(this.terminal, this.ptyWriterId, onBell);
  }

  /** Has `onWrite` take what the program copies to the clipboard with
   *  OSC 52, up to 1 MiB; it returns whether it took it. Reads are always
   *  refused. */
  onClipboardWrite(onWrite: (text: string) => boolean): void {
    this.ensureActive();
    this.clipboardWrite = onWrite;
    this.rt.attachClipboard(this.terminal, this.ptyWriterId, onWrite, CLIPBOARD_MAX_BYTES);
  }

  /** The writer of replies, and the bell, on the terminal again: a reset
   *  or a snapshot replaces what they were attached to. */
  private attachWriters(): void {
    if (!this.ptyWriter || this.disposed) return;
    this.ptyWriterId = this.rt.attachPtyWriter(this.terminal, this.ptyWriter);
    if (this.bell) this.rt.attachBell(this.terminal, this.ptyWriterId, this.bell);
    if (this.clipboardWrite)
      this.rt.attachClipboard(this.terminal, this.ptyWriterId, this.clipboardWrite, CLIPBOARD_MAX_BYTES);
  }

  /** A string the terminal holds, such as its title. */
  private text(data: number): string {
    this.ensureActive();
    const p = this.scratch;
    this.rt.memoryDataView().setUint32(p, 0, true);
    this.rt.memoryDataView().setUint32(p + 4, 0, true);
    if (this.rt.call("ghostty_terminal_get", this.terminal, data, p) !== SUCCESS) return "";
    const ptr = this.rt.memoryDataView().getUint32(p, true);
    const len = this.rt.memoryDataView().getUint32(p + 4, true);
    return len ? decoder.decode(this.rt.bytes(ptr, len)) : "";
  }

  encodeKey(event: KeyboardEvent, action: "press" | "release" = "press"): Uint8Array {
    this.ensureActive();
    this.rt.call("ghostty_key_encoder_set_from_terminal", this.keyEncoder, this.terminal);
    this.rt.call("ghostty_key_event_set_action", this.keyEvent, action === "release" ? 0 : event.repeat ? 2 : 1);
    this.rt.call("ghostty_key_event_set_key", this.keyEvent, ghosttyKeyForEvent(event));
    const mods =
      (event.shiftKey ? 1 : 0) |
      (event.ctrlKey ? 1 << 1 : 0) |
      (event.altKey ? 1 << 2 : 0) |
      (event.metaKey ? 1 << 3 : 0) |
      (event.getModifierState?.("CapsLock") ? 1 << 4 : 0) |
      (event.getModifierState?.("NumLock") ? 1 << 5 : 0);
    this.rt.call("ghostty_key_event_set_mods", this.keyEvent, mods);
    this.rt.call("ghostty_key_event_set_consumed_mods", this.keyEvent, ghosttyConsumedMods(event));
    this.rt.call("ghostty_key_event_set_composing", this.keyEvent, event.isComposing ? 1 : 0);
    this.rt.call(
      "ghostty_key_event_set_unshifted_codepoint",
      this.keyEvent,
      ghosttyUnshiftedCodepoint(event, this.keyboardLayoutMap),
    );
    const text = event.key.length === 1 || [...event.key].length === 1 ? encoder.encode(event.key) : new Uint8Array();
    const textPointer = text.length ? this.rt.alloc(text.length) : 0;
    if (textPointer) this.rt.bytes(textPointer, text.length).set(text);
    // GhosttyString by value is passed by address.
    const str = this.rt.alloc(8);
    this.rt.memoryDataView().setUint32(str, textPointer, true);
    this.rt.memoryDataView().setUint32(str + 4, text.length, true);
    this.rt.call("ghostty_key_event_set_utf8", this.keyEvent, str);
    const out = this.encodeOutput((buf, len, written) =>
      this.rt.call("ghostty_key_encoder_encode", this.keyEncoder, this.keyEvent, buf, len, written),
    );
    // The event keeps the pointer: clear it before freeing the text.
    this.rt.memoryDataView().setUint32(str, 0, true);
    this.rt.memoryDataView().setUint32(str + 4, 0, true);
    this.rt.call("ghostty_key_event_set_utf8", this.keyEvent, str);
    this.rt.free(str, 8);
    if (textPointer) this.rt.free(textPointer, text.length);
    return out;
  }

  encodePaste(text: string): Uint8Array {
    this.ensureActive();
    const input = encoder.encode(text);
    if (!input.length) return input;
    const p = this.rt.alloc(input.length);
    this.rt.bytes(p, input.length).set(input);
    const bracketed = this.mode(2004) ? 1 : 0;
    const out = this.encodeOutput((buf, len, written) =>
      this.rt.call("ghostty_paste_encode", p, input.length, bracketed, buf, len, written),
    );
    this.rt.free(p, input.length);
    return out;
  }

  encodeMouse(input: GhosttyMouseInput): Uint8Array {
    this.ensureActive();
    this.rt.call("ghostty_mouse_encoder_set_from_terminal", this.mouseEncoder, this.terminal);
    const { size: sizeBytes } = this.rt.layout("GhosttyMouseEncoderSize");
    const size = this.workArea(sizeBytes);
    const v = this.rt.memoryDataView();
    const set = (name: string, value: number) =>
      v.setUint32(size + this.rt.field("GhosttyMouseEncoderSize", name).offset, Math.max(0, Math.round(value)), true);
    set("size", sizeBytes);
    set("screen_width", input.screenWidth);
    set("screen_height", input.screenHeight);
    set("cell_width", input.cellWidth);
    set("cell_height", input.cellHeight);
    this.rt.call("ghostty_mouse_encoder_set", this.mouseEncoder, this.k.mouse.size, size);
    this.rt.bytes(this.scratch, 1)[0] = input.anyButtonPressed ? 1 : 0;
    this.rt.call("ghostty_mouse_encoder_set", this.mouseEncoder, this.k.mouse.anyButton, this.scratch);
    this.rt.bytes(this.scratch, 1)[0] = 1;
    this.rt.call("ghostty_mouse_encoder_set", this.mouseEncoder, this.k.mouse.trackLastCell, this.scratch);
    this.rt.call(
      "ghostty_mouse_event_set_action",
      this.mouseEvent,
      input.action === "press" ? 0 : input.action === "release" ? 1 : 2,
    );
    if (input.button === null) this.rt.call("ghostty_mouse_event_clear_button", this.mouseEvent);
    else this.rt.call("ghostty_mouse_event_set_button", this.mouseEvent, input.button);
    this.rt.call("ghostty_mouse_event_set_mods", this.mouseEvent, input.mods);
    const pos = this.workArea(8);
    this.rt.memoryDataView().setFloat32(pos, input.x, true);
    this.rt.memoryDataView().setFloat32(pos + 4, input.y, true);
    this.rt.call("ghostty_mouse_event_set_position", this.mouseEvent, pos);
    return this.encodeOutput((buf, len, written) =>
      this.rt.call("ghostty_mouse_encoder_encode", this.mouseEncoder, this.mouseEvent, buf, len, written),
    );
  }

  // ------------------------------------------------------------ selection

  /** A grid reference (8 bytes) for a point in the viewport, or 0. */
  private gridRef(col: number, row: number, tag = this.k.point.viewport): number {
    const pointSize = this.rt.layout("GhosttyPoint").size;
    const point = this.rt.alloc(pointSize);
    const v = this.rt.memoryDataView();
    const value = this.rt.field("GhosttyPoint", "value").offset;
    v.setUint32(point, tag, true);
    v.setUint16(point + value, Math.max(0, col), true);
    v.setUint32(point + value + 4, Math.max(0, row), true);
    const ref = this.rt.alloc(8);
    const result = this.rt.call("ghostty_terminal_grid_ref", this.terminal, point, ref);
    this.rt.free(point, pointSize);
    if (result !== SUCCESS) {
      this.rt.free(ref, 8);
      return 0;
    }
    return ref;
  }

  /** Selects from one cell to another, inclusive. The rows are the
   *  viewport's, or with `screen` the whole screen's, scrollback included,
   *  which stay put while the viewport scrolls. A rectangle selects the
   *  same columns of every row. */
  setSelection(
    anchor: { x: number; y: number },
    end: { x: number; y: number },
    { screen = false, rectangle = false } = {},
  ): void {
    this.ensureActive();
    const size = this.rt.layout("GhosttySelection").size;
    const sel = this.rt.alloc(size);
    const tag = screen ? this.k.point.screen : this.k.point.viewport;
    const a = this.gridRef(anchor.x, anchor.y, tag);
    const b = this.gridRef(end.x, end.y, tag);
    if (a && b) {
      this.rt.memoryBytes().copyWithin(sel, a, a + 8);
      this.rt.memoryBytes().copyWithin(sel + 8, b, b + 8);
      this.rt.memoryBytes()[sel + this.rt.field("GhosttySelection", "rectangle").offset] = rectangle ? 1 : 0;
      this.rt.call("ghostty_terminal_set", this.terminal, this.k.opt.selection, sel);
    }
    this.rt.free(a, 8);
    this.rt.free(b, 8);
    this.rt.free(sel, size);
  }

  clearSelection(): void {
    this.ensureActive();
    this.rt.call("ghostty_terminal_set", this.terminal, this.k.opt.selection, 0);
  }

  selectWord(col: number, row: number): void {
    this.selectAt("GhosttyTerminalSelectWordOptions", "ghostty_terminal_select_word", col, row);
  }

  selectLine(col: number, row: number): void {
    this.selectAt("GhosttyTerminalSelectLineOptions", "ghostty_terminal_select_line", col, row);
  }

  private selectAt(optionsName: string, fn: string, col: number, row: number): void {
    this.ensureActive();
    const { size } = this.rt.layout(optionsName);
    const options = this.rt.alloc(size);
    const selSize = this.rt.layout("GhosttySelection").size;
    const sel = this.rt.alloc(selSize);
    const ref = this.gridRef(col, row);
    if (ref) {
      this.rt.memoryDataView().setUint32(options, size, true);
      const refOffset = this.rt.field(optionsName, "ref").offset;
      this.rt.memoryBytes().copyWithin(options + refOffset, ref, ref + 8);
      if (this.rt.call(fn, this.terminal, options, sel) === SUCCESS)
        this.rt.call("ghostty_terminal_set", this.terminal, this.k.opt.selection, sel);
      this.rt.free(ref, 8);
    }
    this.rt.free(sel, selSize);
    this.rt.free(options, size);
  }

  hasSelection(): boolean {
    this.ensureActive();
    const size = this.rt.layout("GhosttySelection").size;
    const sel = this.rt.alloc(size);
    const r = this.rt.call("ghostty_terminal_get", this.terminal, this.k.data.selection, sel);
    this.rt.free(sel, size);
    return r === SUCCESS;
  }

  /** The selection as plain text, soft-wrapped lines joined and trailing blanks trimmed. */
  selectionText(): string | null {
    this.ensureActive();
    const size = this.rt.layout("GhosttySelection").size;
    const sel = this.rt.alloc(size);
    let formatter = 0;
    try {
      if (this.rt.call("ghostty_terminal_get", this.terminal, this.k.data.selection, sel) !== SUCCESS) return null;
      formatter = this.newHandle("ghostty_formatter_terminal_new", this.terminal);
      this.rt.bytes(this.scratch, 1)[0] = 1;
      this.rt.call("ghostty_formatter_set", formatter, this.k.formatter.unwrap, this.scratch);
      this.rt.call("ghostty_formatter_set", formatter, this.k.formatter.trim, this.scratch);
      this.rt.call("ghostty_formatter_set", formatter, this.k.formatter.selection, sel);
      const f = formatter;
      return decoder.decode(
        this.encodeOutput((buf, len, written) => this.rt.call("ghostty_formatter_format_buf", f, buf, len, written)),
      );
    } finally {
      if (formatter) this.rt.call("ghostty_formatter_free", formatter);
      this.rt.free(sel, size);
    }
  }

  hyperlinkAt(col: number, row: number): string | null {
    this.ensureActive();
    const ref = this.gridRef(col, row);
    if (!ref) return null;
    const out = this.encodeOutput((buf, len, written) =>
      this.rt.call("ghostty_grid_ref_hyperlink_uri", ref, buf, len, written),
    );
    this.rt.free(ref, 8);
    return out.length ? decoder.decode(out) : null;
  }

  // --------------------------------------------------------------- search

  /** Counts matches of a literal needle (ASCII letters ignore case), and
   *  selects the next one, older first, as the terminal's selection,
   *  scrolled into view. */
  search(query: string, backwards = false): number {
    this.ensureActive();
    if (!this.searchHandle) this.searchHandle = this.newHandle("ghostty_search_new", this.terminal);
    if (query !== this.searchNeedle) {
      const bytes = encoder.encode(query);
      const p = this.rt.alloc(Math.max(1, bytes.length));
      this.rt.bytes(p, bytes.length).set(bytes);
      const str = this.rt.alloc(8);
      this.rt.memoryDataView().setUint32(str, p, true);
      this.rt.memoryDataView().setUint32(str + 4, bytes.length, true);
      this.rt.call("ghostty_search_set", this.searchHandle, this.k.search.needle, str);
      this.rt.free(str, 8);
      this.rt.free(p, Math.max(1, bytes.length));
      this.searchNeedle = query;
    }
    this.ok("search_run", this.rt.call("ghostty_search_run", this.searchHandle));
    this.rt.memoryDataView().setUint32(this.scratch, 0, true);
    this.rt.call("ghostty_search_get", this.searchHandle, this.k.search.total, this.scratch);
    const total = this.rt.memoryDataView().getUint32(this.scratch, true);
    if (total) {
      this.rt.call(backwards ? "ghostty_search_select_prev" : "ghostty_search_select_next", this.searchHandle);
      const size = this.rt.layout("GhosttySelection").size;
      const sel = this.rt.alloc(size);
      if (this.rt.call("ghostty_search_get", this.searchHandle, this.k.search.selected, sel) === SUCCESS)
        this.rt.call("ghostty_terminal_set", this.terminal, this.k.opt.selection, sel);
      this.rt.free(sel, size);
    }
    return total;
  }

  /** The cells of the last search's matches in the viewport, a span per
   *  row. */
  searchMatchesInView(): { row: number; start: number; end: number }[] {
    this.ensureActive();
    if (!this.searchHandle || !this.searchNeedle) return [];
    const buffer = this.rt.alloc(12);
    const v = this.rt.memoryDataView();
    v.setUint32(buffer, 0, true);
    v.setUint32(buffer + 4, 0, true);
    v.setUint32(buffer + 8, 0, true);
    // asked with no room, it says how much it needs
    this.rt.call("ghostty_search_get", this.searchHandle, this.k.search.inView, buffer);
    const count = v.getUint32(buffer + 8, true);
    const size = this.rt.layout("GhosttySelection").size;
    const spans: { row: number; start: number; end: number }[] = [];
    if (count) {
      const list = this.rt.alloc(count * size);
      const view = this.rt.memoryDataView();
      view.setUint32(buffer, list, true);
      view.setUint32(buffer + 4, count, true);
      if (this.rt.call("ghostty_search_get", this.searchHandle, this.k.search.inView, buffer) === SUCCESS) {
        const { cols, rows } = this.size();
        for (let i = 0; i < count; i++) {
          const start = this.inViewport(list + i * size);
          const end = this.inViewport(list + i * size + 8);
          // a match partly out of view is cut at the viewport's edge
          if (!start && !end) continue;
          const from = start ?? { x: 0, y: 0 };
          const to = end ?? { x: cols - 1, y: rows - 1 };
          for (let y = from.y; y <= to.y; y++) {
            spans.push({ row: y, start: y === from.y ? from.x : 0, end: y === to.y ? to.x + 1 : cols });
          }
        }
      }
      this.rt.free(list, count * size);
    }
    this.rt.free(buffer, 12);
    return spans;
  }

  /** A grid reference's cell in the viewport, or null when out of view. */
  private inViewport(ref: number): { x: number; y: number } | null {
    const out = this.workArea(8);
    const ok =
      this.rt.call("ghostty_terminal_point_from_grid_ref", this.terminal, ref, this.k.point.viewport, out) === SUCCESS;
    const v = this.rt.memoryDataView();
    const point = ok ? { x: v.getUint16(out, true), y: v.getUint32(out + 4, true) } : null;
    return point && point.y < this.size().rows ? point : null;
  }

  /** Ends the search: no matches are highlighted any more. */
  clearSearch(): void {
    this.freeSearch();
  }

  /** The text of every row of the screen, scrollback first, a line per
   *  row with trailing blanks trimmed: line i is screen row i. */
  screenLines(): string[] {
    this.ensureActive();
    const formatter = this.newHandle("ghostty_formatter_terminal_new", this.terminal);
    try {
      this.rt.bytes(this.scratch, 1)[0] = 1;
      this.rt.call("ghostty_formatter_set", formatter, this.k.formatter.trim, this.scratch);
      return decoder
        .decode(
          this.encodeOutput((buf, len, written) =>
            this.rt.call("ghostty_formatter_format_buf", formatter, buf, len, written),
          ),
        )
        .split("\n");
    } finally {
      this.rt.call("ghostty_formatter_free", formatter);
    }
  }

  /** Scrolls the viewport, if needed, so that a screen row is in it. */
  scrollToScreenRow(row: number): void {
    const { offset, length } = this.scrollbar();
    if (row < offset) this.scroll(row - offset);
    else if (row >= offset + length) this.scroll(row - offset - length + 1);
  }

  private freeSearch(): void {
    if (this.searchHandle) this.rt.call("ghostty_search_free", this.searchHandle);
    this.searchHandle = 0;
    this.searchNeedle = "";
  }

  // ------------------------------------------------------------- snapshot

  snapshot(): GhosttySnapshot {
    this.ensureActive();
    this.palette = null;
    const rs = this.k.rs;
    this.ok("render_state_update", this.rt.call("ghostty_render_state_update", this.renderState, this.terminal));
    const cols = this.rsU16(rs.cols);
    const rowCount = this.rsU16(rs.rows);
    const dirty = this.rsU32(rs.dirty);
    const foreground = this.rsColor(rs.foreground, { r: 229, g: 231, b: 235 });
    const background = this.rsColor(rs.background, { r: 0, g: 0, b: 0 });
    const cursor = this.rsBool(rs.cursorHasValue) ? this.rsColor(rs.cursor, foreground) : foreground;
    const inViewport = this.rsBool(rs.cursorInViewport);
    const cursorVisible = this.rsBool(rs.cursorVisible) && inViewport;
    const cursorX = inViewport ? this.rsU16(rs.cursorX) : -1;
    const cursorY = inViewport ? this.rsU16(rs.cursorY) : -1;

    if (this.rows.length !== rowCount || this.rows.some((r) => r.cells.length !== cols)) {
      this.rows = Array.from({ length: rowCount }, () => ({
        cells: Array.from({ length: cols }, () => emptyCell(foreground, background)),
        text: "",
        isWrapContinuation: false,
        wrapsToNext: false,
      }));
    }
    const dirtyRows = new Set<number>();
    if (dirty !== 0) {
      this.ok(
        "render_state_get(row iterator)",
        this.rt.call("ghostty_render_state_get", this.renderState, rs.rowIterator, this.rowIteratorSlot),
      );
      const iterator = this.rt.readPointer(this.rowIteratorSlot);
      const next = this.rt.fn("ghostty_render_state_row_iterator_next");
      let y = 0;
      while (y < rowCount && next(iterator) !== 0) {
        if (dirty === rs.dirtyFull || this.rowBool(iterator, this.k.row.dirty)) {
          this.rows[y] = this.readRow(iterator, cols, foreground, background);
          dirtyRows.add(y);
          this.rt.memoryBytes()[this.scratch] = 0;
          this.rt.call("ghostty_render_state_row_set", iterator, rs.rowOptDirty, this.scratch);
        }
        y += 1;
      }
      this.rt.memoryDataView().setUint32(this.scratch, 0, true);
      this.rt.call("ghostty_render_state_set", this.renderState, rs.optDirty, this.scratch);
    }
    return {
      cols,
      rows: rowCount,
      foreground,
      background,
      cursor,
      cursorX,
      cursorY,
      cursorVisible,
      cursorBlinking: this.rsBool(rs.cursorBlinking),
      cursorStyle: this.rsU32(rs.cursorStyle),
      dirtyRows,
      rowData: this.rows,
    };
  }

  /** The underline colour of the cell whose style was just read: its
   *  RGB, or the palette's colour at its index, or null for the text's. */
  private underlineColor(): GhosttyColor | null {
    const v = this.rt.memoryDataView();
    // GhosttyStyle.underline_color: a tag, then its value
    const at = this.style + 8;
    switch (v.getUint8(at)) {
      case 2:
        return readColor(v, at + 1);
      case 1: {
        const index = v.getUint8(at + 1);
        const palette = this.readPalette();
        return { r: palette[index * 3], g: palette[index * 3 + 1], b: palette[index * 3 + 2] };
      }
      default:
        return null;
    }
  }

  /** The terminal's 256 colours, read once a snapshot. */
  private readPalette(): Uint8Array {
    if (this.palette) return this.palette;
    const size = 256 * 3;
    const pointer = this.rt.alloc(size);
    this.rt.call("ghostty_terminal_get", this.terminal, this.k.data.palette, pointer);
    this.palette = this.rt.bytes(pointer, size).slice();
    this.rt.free(pointer, size);
    return this.palette;
  }

  private readRow(iterator: number, cols: number, defaultFg: GhosttyColor, defaultBg: GhosttyColor): GhosttyRow {
    const rt = this.rt;
    this.ok("row_get(raw)", rt.call("ghostty_render_state_row_get", iterator, this.k.row.raw, this.scratch));
    const rawRow = rt.memoryDataView().getBigUint64(this.scratch, true);
    rt.memoryBytes()[this.scratch + 8] = 0;
    rt.call("ghostty_row_get", rawRow, this.k.rawRow.wrapContinuation, this.scratch + 8);
    const isWrapContinuation = rt.memoryBytes()[this.scratch + 8] !== 0;
    rt.memoryBytes()[this.scratch + 8] = 0;
    rt.call("ghostty_row_get", rawRow, this.k.rawRow.wrap, this.scratch + 8);
    const wrapsToNext = rt.memoryBytes()[this.scratch + 8] !== 0;

    this.ok("row_get(cells)", rt.call("ghostty_render_state_row_get", iterator, this.k.row.cells, this.rowCellsSlot));
    const cellsIterator = rt.readPointer(this.rowCellsSlot);
    const cellsNext = rt.fn("ghostty_render_state_row_cells_next");
    const getMulti = rt.fn("ghostty_render_state_row_cells_get_multi");
    const cellGet = rt.fn("ghostty_render_state_row_cells_get");
    const flagsOffset = 12; // GhosttyStyle.flags
    const cells: GhosttyCell[] = [];
    const written = this.query + 40;
    while (cells.length < cols && cellsNext(cellsIterator) !== 0) {
      let v = rt.memoryDataView();
      v.setUint32(this.utf8Desc, this.utf8, true);
      v.setUint32(this.utf8Desc + 4, this.utf8Capacity, true);
      v.setUint32(this.utf8Desc + 8, 0, true);
      let result = getMulti(cellsIterator, QUERY_KEYS, this.query, this.query + 20, written);
      v = rt.memoryDataView();
      let n = v.getUint32(written, true);
      if (n === 2 && result === OUT_OF_SPACE) {
        // A grapheme cluster longer than the buffer: grow it and read the rest.
        this.growUtf8(Math.max(v.getUint32(this.utf8Desc + 8, true), this.utf8Capacity * 2));
        v = rt.memoryDataView();
        v.setUint32(this.utf8Desc, this.utf8, true);
        v.setUint32(this.utf8Desc + 4, this.utf8Capacity, true);
        v.setUint32(this.utf8Desc + 8, 0, true);
        result = getMulti(cellsIterator, QUERY_KEYS, this.query, this.query + 20, written);
        v = rt.memoryDataView();
        n = v.getUint32(written, true);
      }
      if (n < 3) this.ok("row_cells_get_multi", result);
      const selected = v.getUint8(this.scratch + 16) !== 0;
      const textLen = v.getUint32(this.utf8Desc + 8, true);
      // most cells hold one ASCII character, which needs no decoder
      const first = textLen === 1 ? rt.memoryBytes()[this.utf8] : 0x80;
      const text = !textLen ? "" : first < 0x80 ? ASCII[first]! : decoder.decode(rt.bytes(this.utf8, textLen));
      let background = n === QUERY_KEYS ? readColor(v, this.scratch + 20) : defaultBg;
      // GhosttyStyle.fg_color: a tag, then its value
      let foreground: GhosttyColor;
      switch (v.getUint8(this.style)) {
        case 2:
          foreground = readColor(v, this.style + 1);
          break;
        case 1:
          foreground = paletteColor(this.readPalette(), v.getUint8(this.style + 1));
          break;
        default:
          foreground = defaultFg;
      }
      let wide = 0;
      // A wrap spacer in the final column can follow a spacer tail.
      if (!text && (cells.at(-1)?.text.length || cells.length === cols - 1)) {
        if (cellGet(cellsIterator, this.k.cell.raw, this.scratch) === SUCCESS) {
          const rawCell = rt.memoryDataView().getBigUint64(this.scratch, true);
          rt.memoryDataView().setUint32(this.scratch + 8, 0, true);
          rt.call("ghostty_cell_get", rawCell, this.k.rawCellWide, this.scratch + 8);
          wide = rt.memoryDataView().getUint32(this.scratch + 8, true);
        }
      }
      const flags = rt.memoryDataView().getUint16(this.style + flagsOffset, true);
      if (flags & (1 << 4)) [foreground, background] = [background, foreground];
      if (flags & (1 << 2)) foreground = blend(foreground, background);
      cells.push({
        text,
        wide,
        foreground,
        background,
        bold: (flags & 1) !== 0,
        italic: (flags & 2) !== 0,
        invisible: (flags & (1 << 5)) !== 0,
        strikethrough: (flags & (1 << 6)) !== 0,
        overline: (flags & (1 << 7)) !== 0,
        underline: (flags >> 8) & 7,
        underlineColor: this.underlineColor(),
        selected,
      });
    }
    while (cells.length < cols) cells.push(emptyCell(defaultFg, defaultBg));
    // the row's text, which only the screen reader mirror reads, is built
    // when it is first read
    let text: string | null = null;
    return {
      cells,
      get text() {
        text ??= cells
          .map((c) => c.text || " ")
          .join("")
          .trimEnd();
        return text;
      },
      isWrapContinuation,
      wrapsToNext,
    };
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    const rt = this.rt;
    this.freeSearch();
    if (this.mouseEvent) rt.call("ghostty_mouse_event_free", this.mouseEvent);
    if (this.mouseEncoder) rt.call("ghostty_mouse_encoder_free", this.mouseEncoder);
    if (this.keyEvent) rt.call("ghostty_key_event_free", this.keyEvent);
    if (this.keyEncoder) rt.call("ghostty_key_encoder_free", this.keyEncoder);
    if (this.rowCellsSlot) {
      const cells = rt.readPointer(this.rowCellsSlot);
      if (cells) rt.call("ghostty_render_state_row_cells_free", cells);
      rt.freeOpaque(this.rowCellsSlot);
    }
    if (this.rowIteratorSlot) {
      const it = rt.readPointer(this.rowIteratorSlot);
      if (it) rt.call("ghostty_render_state_row_iterator_free", it);
      rt.freeOpaque(this.rowIteratorSlot);
    }
    if (this.renderState) rt.call("ghostty_render_state_free", this.renderState);
    if (this.terminal) {
      if (this.ptyWriterId) rt.detachPtyWriter(this.terminal, this.ptyWriterId);
      rt.call("ghostty_terminal_free", this.terminal);
    }
    if (this.style) rt.free(this.style, rt.layout("GhosttyStyle").size);
    if (this.utf8Desc) rt.free(this.utf8Desc, rt.layout("GhosttyBuffer").size);
    if (this.utf8) rt.free(this.utf8, this.utf8Capacity);
    if (this.query) rt.free(this.query, 64);
    if (this.work) rt.free(this.work, WORK_SIZE);
    if (this.input) rt.free(this.input, this.inputCapacity);
    if (this.scratch) rt.free(this.scratch, 32);
  }

  // --------------------------------------------------------------- helpers

  /** Runs an encoder twice: once for the size, once into a buffer of that size. */
  private encodeOutput(encode: (buf: number, len: number, written: number) => number): Uint8Array {
    const written = this.rt.alloc(4);
    try {
      const first = encode(0, 0, written);
      const size = this.rt.memoryDataView().getUint32(written, true);
      if (first !== OUT_OF_SPACE || size === 0) return new Uint8Array();
      const buf = this.rt.alloc(size);
      const result = encode(buf, size, written);
      const len = this.rt.memoryDataView().getUint32(written, true);
      const out = result === SUCCESS ? this.rt.memoryBytes().slice(buf, buf + len) : new Uint8Array();
      this.rt.free(buf, size);
      return out;
    } finally {
      this.rt.free(written, 4);
    }
  }

  private rsGet(data: number): number {
    return this.rt.call("ghostty_render_state_get", this.renderState, data, this.scratch);
  }

  private rsU16(data: number): number {
    this.rt.memoryDataView().setUint32(this.scratch, 0, true);
    this.ok("render_state_get", this.rsGet(data));
    return this.rt.memoryDataView().getUint16(this.scratch, true);
  }

  private rsU32(data: number): number {
    this.rt.memoryDataView().setUint32(this.scratch, 0, true);
    this.ok("render_state_get", this.rsGet(data));
    return this.rt.memoryDataView().getUint32(this.scratch, true);
  }

  private rsBool(data: number): boolean {
    this.rt.memoryBytes()[this.scratch] = 0;
    const r = this.rsGet(data);
    if (r !== SUCCESS && r !== NO_VALUE) this.ok("render_state_get", r);
    return r === SUCCESS && this.rt.memoryBytes()[this.scratch] !== 0;
  }

  private rsColor(data: number, fallback: GhosttyColor): GhosttyColor {
    this.rt.memoryBytes().fill(0, this.scratch, this.scratch + 3);
    return this.rsGet(data) === SUCCESS ? readColor(this.rt.memoryDataView(), this.scratch) : fallback;
  }

  private rowBool(iterator: number, data: number): boolean {
    this.rt.memoryBytes()[this.scratch] = 0;
    return (
      this.rt.call("ghostty_render_state_row_get", iterator, data, this.scratch) === SUCCESS &&
      this.rt.memoryBytes()[this.scratch] !== 0
    );
  }

  private ok(operation: string, result: number): void {
    if (result !== SUCCESS) throw new Error(`${operation} failed with result ${result}`);
  }

  private ensureActive(): void {
    if (this.disposed) throw new Error("libghostty-vt terminal has been disposed");
  }
}

function clamp16(n: number): number {
  return Math.max(1, Math.min(65_535, Math.floor(n)));
}

/** The input buffer's first size, and the largest write it takes: the
 *  window's writes are pieces of up to 64 KiB. */
const INPUT_INITIAL = 4096;
const INPUT_LIMIT = 256 * 1024;
/** The bytes of the work area. */
const WORK_SIZE = 64;

/** How many values the cell query reads. */
const QUERY_KEYS = 4;

function paletteColor(palette: Uint8Array, index: number): GhosttyColor {
  return { r: palette[index * 3]!, g: palette[index * 3 + 1]!, b: palette[index * 3 + 2]! };
}

function readColor(v: DataView, p: number): GhosttyColor {
  return { r: v.getUint8(p), g: v.getUint8(p + 1), b: v.getUint8(p + 2) };
}

function emptyCell(foreground: GhosttyColor, background: GhosttyColor): GhosttyCell {
  return {
    text: "",
    wide: 0,
    foreground,
    background,
    bold: false,
    italic: false,
    invisible: false,
    strikethrough: false,
    overline: false,
    underline: 0,
    underlineColor: null,
    selected: false,
  };
}
