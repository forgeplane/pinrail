// libghostty-vt as WebAssembly: compiled once per window from the file the
// app serves, and instantiated for each terminal with a memory of its own.
// It wraps the C ABI for core.ts: struct layouts and enum values come from
// the library's own ghostty_type_json, looked up by name, and the replies
// the terminal writes to its PTY reach JavaScript through a trampoline.
//
// Adapted from T3 Code (https://github.com/pingdotgg/t3code), MIT licence,
// Copyright (c) 2026 T3 Tools Inc., apps/web/src/terminal/ghostty/runtime.ts
// at commit ade47301, ported to the libghostty-vt 1.0 API. The licence is in
// licenses/t3code.txt.

/** Where the app serves the build scripts/ghostty-wasm.sh makes. */
const WASM_URL = "/ghostty/ghostty-vt.wasm";

type WasmFunction = (...args: Array<number | bigint>) => number;

interface TypeField {
  readonly offset: number;
  readonly size: number;
  readonly type: string;
}

interface TypeInfo {
  readonly kind: string;
  readonly size: number;
  readonly fields?: Readonly<Record<string, TypeField>>;
  readonly values?: Readonly<Record<string, number>>;
}

interface TypeJson {
  readonly library_version: string;
  readonly types: Readonly<Record<string, TypeInfo>>;
}

const textDecoder = new TextDecoder();

/** The length-prefixed bytes of a name in a WebAssembly module. */
const name = (text: string) => [text.length, ...new TextEncoder().encode(text)];

/** A section of a WebAssembly module: its id, its size, its bytes. Each
 *  here is under 128 bytes, so its size is one byte. */
const section = (id: number, bytes: number[]) => [id, bytes.length, ...bytes];

/**
 * A module of one function, `callback`, of `params` i32 arguments, that
 * calls the imported `env.callback` with the same arguments. A JavaScript
 * function cannot go into the library's function table, a WebAssembly one
 * can: the library calls this one through the table, and it calls
 * JavaScript.
 */
function trampoline(params: number): Uint8Array<ArrayBuffer> {
  const i32 = 0x7f;
  return new Uint8Array([
    // the magic number and version 1
    0x00,
    0x61,
    0x73,
    0x6d,
    0x01,
    0x00,
    0x00,
    0x00,
    // types: one, a function of `params` i32 that returns nothing
    ...section(1, [1, 0x60, params, ...Array<number>(params).fill(i32), 0]),
    // imports: env.callback, a function of that type
    ...section(2, [1, ...name("env"), ...name("callback"), 0x00, 0]),
    // functions: one of that type
    ...section(3, [1, 0]),
    // exports: it, as callback, function 1 after the import
    ...section(7, [1, ...name("callback"), 0x00, 1]),
    // code: one body, of no locals, local.get each argument, call the
    // import and end: 4 bytes and 2 for each argument
    ...section(10, [
      1,
      4 + 2 * params,
      0,
      ...Array.from({ length: params }, (_, n) => [0x20, n]).flat(),
      0x10,
      0,
      0x0b,
    ]),
  ]);
}

let compiled: Promise<WebAssembly.Module> | null = null;
/** The trampoline modules, compiled once per window, by their number of
 *  arguments. */
const trampolines = new Map<number, WebAssembly.Module>();
/** The library's types, read once per compiled library: each instance of
 *  it reports the same. */
const typesOf = new WeakMap<WebAssembly.Module, TypeJson>();

/** The library, compiled once per window. It fails when the window cannot
 *  run WebAssembly, or its CSP forbids compiling it. */
export function compileGhostty(): Promise<WebAssembly.Module> {
  compiled ??= (async () => {
    if (typeof WebAssembly === "undefined") throw new Error("this window cannot run WebAssembly");
    const response = await fetch(WASM_URL);
    if (!response.ok) throw new Error(`libghostty-vt could not be loaded (${response.status})`);
    return WebAssembly.compileStreaming
      ? WebAssembly.compileStreaming(response)
      : WebAssembly.compile(await response.arrayBuffer());
  })().catch((error: unknown) => {
    compiled = null;
    throw error;
  });
  return compiled;
}

/** One instance of the library, for one terminal. */
export class GhosttyRuntime {
  readonly memory: WebAssembly.Memory;
  readonly version: string;
  private readonly types: TypeJson["types"];
  private readonly exports: WebAssembly.Exports;
  private memoryView: DataView;
  private memoryU8: Uint8Array;
  private readonly ptyWriters = new Map<number, (data: Uint8Array) => void>();
  private readonly bells = new Map<number, () => void>();
  /** What copies a program's clipboard writes, by the terminal's writer
   *  id: true when it took the text. */
  private readonly clipboards = new Map<number, (text: string) => boolean>();
  private nextPtyWriterId = 1;
  private writePtyFunctionIndex = 0;
  private bellFunctionIndex = 0;
  private clipboardFunctionIndex = 0;

  private constructor(module: WebAssembly.Module, instance: WebAssembly.Instance) {
    this.exports = instance.exports;
    const memory = instance.exports.memory;
    if (!(memory instanceof WebAssembly.Memory)) throw new Error("libghostty-vt did not export its memory");
    this.memory = memory;
    this.memoryView = new DataView(memory.buffer);
    this.memoryU8 = new Uint8Array(memory.buffer);
    let json = typesOf.get(module);
    if (!json) {
      const pointer = this.call("ghostty_type_json");
      let end = pointer;
      while (this.memoryU8[end] !== 0) end += 1;
      json = JSON.parse(textDecoder.decode(this.memoryU8.subarray(pointer, end))) as TypeJson;
      typesOf.set(module, json);
    }
    this.types = json.types;
    this.version = json.library_version;
  }

  /** A new instance of the compiled library, with its own memory. */
  static async instantiate(): Promise<GhosttyRuntime> {
    const module = await compileGhostty();
    const instance = await WebAssembly.instantiate(module, {});
    const runtime = new GhosttyRuntime(module, instance);
    if (!runtime.version.startsWith("1.0.")) throw new Error(`expected libghostty-vt 1.0, got ${runtime.version}`);
    runtime.installTrampolines();
    return runtime;
  }

  call(name: string, ...args: Array<number | bigint>): number {
    return this.fn(name)(...args);
  }

  fn(name: string): WasmFunction {
    const fn = this.exports[name];
    if (typeof fn !== "function") throw new Error(`libghostty-vt has no export ${name}`);
    return fn as WasmFunction;
  }

  layout(name: string): { size: number; fields: Readonly<Record<string, TypeField>> } {
    const t = this.types[name];
    if (!t?.fields) throw new Error(`libghostty-vt has no struct ${name}`);
    return { size: t.size, fields: t.fields };
  }

  field(struct: string, name: string): TypeField {
    const f = this.layout(struct).fields[name];
    if (!f) throw new Error(`libghostty-vt has no field ${struct}.${name}`);
    return f;
  }

  /** An enum value by its name without the prefix: enumValue("GhosttyTerminalOption", "WRITE_PTY"). */
  enumValue(type: string, key: string): number {
    const v = this.types[type]?.values?.[key];
    if (v === undefined) throw new Error(`libghostty-vt has no value ${type}.${key}`);
    return v;
  }

  alloc(size: number): number {
    const pointer = this.call("ghostty_wasm_alloc", size);
    if (pointer === 0) throw new Error(`libghostty-vt could not allocate ${size} bytes`);
    this.memoryBytes().fill(0, pointer, pointer + size);
    return pointer;
  }

  free(pointer: number, size: number): void {
    if (pointer !== 0) this.call("ghostty_wasm_free", pointer, size);
  }

  allocOpaque(): number {
    const pointer = this.call("ghostty_wasm_alloc_opaque");
    if (pointer === 0) throw new Error("libghostty-vt could not allocate a pointer");
    this.memoryDataView().setUint32(pointer, 0, true);
    return pointer;
  }

  freeOpaque(pointer: number): void {
    if (pointer !== 0) this.call("ghostty_wasm_free_opaque", pointer);
  }

  readPointer(slot: number): number {
    return this.memoryDataView().getUint32(slot, true);
  }

  /** Has `writer` receive what `terminal` writes to its PTY: the replies
   *  to the program's queries. */
  attachPtyWriter(terminal: number, writer: (data: Uint8Array) => void): number {
    const id = this.nextPtyWriterId++;
    this.ptyWriters.set(id, writer);
    // pointer-typed options take the value itself, not a pointer to it
    this.call("ghostty_terminal_set", terminal, this.enumValue("GhosttyTerminalOption", "USERDATA"), id);
    this.call(
      "ghostty_terminal_set",
      terminal,
      this.enumValue("GhosttyTerminalOption", "WRITE_PTY"),
      this.writePtyFunctionIndex,
    );
    return id;
  }

  detachPtyWriter(terminal: number, id: number): void {
    this.call("ghostty_terminal_set", terminal, this.enumValue("GhosttyTerminalOption", "BELL"), 0);
    this.bells.delete(id);
    this.call("ghostty_terminal_set", terminal, this.enumValue("GhosttyTerminalOption", "CLIPBOARD_WRITE"), 0);
    this.clipboards.delete(id);
    this.call("ghostty_terminal_set", terminal, this.enumValue("GhosttyTerminalOption", "WRITE_PTY"), 0);
    this.call("ghostty_terminal_set", terminal, this.enumValue("GhosttyTerminalOption", "USERDATA"), 0);
    this.ptyWriters.delete(id);
  }

  bytes(pointer: number, size: number): Uint8Array {
    return new Uint8Array(this.memoryDataView().buffer, pointer, size);
  }

  memoryDataView(): DataView {
    this.refreshMemoryViews();
    return this.memoryView;
  }

  memoryBytes(): Uint8Array {
    this.refreshMemoryViews();
    return this.memoryU8;
  }

  private refreshMemoryViews(): void {
    // growing the memory detaches the old buffer
    if (this.memoryU8.byteLength === 0) {
      this.memoryView = new DataView(this.memory.buffer);
      this.memoryU8 = new Uint8Array(this.memory.buffer);
    }
  }

  /** Has `onBell` called when the program rings `terminal`'s bell; the
   *  terminal's PTY writer must be attached, whose id it shares. */
  attachBell(terminal: number, id: number, onBell: () => void): void {
    this.bells.set(id, onBell);
    this.call(
      "ghostty_terminal_set",
      terminal,
      this.enumValue("GhosttyTerminalOption", "BELL"),
      this.bellFunctionIndex,
    );
  }

  /** Has `onWrite` take the text a program copies to the clipboard (OSC
   *  52), up to `maxBytes`; the terminal's PTY writer must be attached,
   *  whose id it shares. A program can never read the clipboard, as no
   *  reader is set. */
  attachClipboard(terminal: number, id: number, onWrite: (text: string) => boolean, maxBytes: number): void {
    this.clipboards.set(id, onWrite);
    const size = this.alloc(4);
    this.memoryDataView().setUint32(size, maxBytes, true);
    this.call(
      "ghostty_terminal_set",
      terminal,
      this.enumValue("GhosttyTerminalOption", "CLIPBOARD_WRITE_MAX_BYTES"),
      size,
    );
    this.free(size, 4);
    this.call(
      "ghostty_terminal_set",
      terminal,
      this.enumValue("GhosttyTerminalOption", "CLIPBOARD_WRITE"),
      this.clipboardFunctionIndex,
    );
  }

  /** Answers a clipboard write (GhosttyClipboardWrite, 32 bytes on wasm32)
   *  through its reply function: the plain text goes to the terminal's
   *  handler, for the standard clipboard only. */
  private clipboardWrite(userdata: number, write: number): void {
    const v = this.memoryDataView();
    const field = (name: string) => write + this.field("GhosttyClipboardWrite", name).offset;
    const location = v.getUint32(field("location"), true);
    const contents = v.getUint32(field("contents"), true);
    const count = v.getUint32(field("contents_len"), true);
    const string = (at: number) => {
      const view = this.memoryDataView();
      const pointer = view.getUint32(at, true);
      const length = view.getUint32(at + 4, true);
      return new TextDecoder().decode(this.memoryBytes().slice(pointer, pointer + length));
    };
    const results = { success: 0, denied: 1, unsupported: 2 };
    let result = results.unsupported;
    if (location === this.enumValue("GhosttyClipboardLocation", "STANDARD")) {
      const contentSize = this.layout("GhosttyClipboardContent").size;
      const dataOffset = this.field("GhosttyClipboardContent", "data").offset;
      for (let i = 0; i < count; i++) {
        const content = contents + i * contentSize;
        if (!string(content).startsWith("text/plain")) continue;
        const handler = this.clipboards.get(userdata);
        result = handler?.(string(content + dataOffset)) ? results.success : results.denied;
        break;
      }
    }
    const replySize = this.layout("GhosttyClipboardWriteReply").size;
    const reply = this.alloc(replySize);
    const r = this.memoryDataView();
    r.setUint32(reply, replySize, true);
    r.setUint32(reply + this.field("GhosttyClipboardWriteReply", "result").offset, result, true);
    const table = this.exports.__indirect_function_table as WebAssembly.Table;
    const replyFn = table.get(this.memoryDataView().getUint32(field("reply"), true)) as (w: number, r: number) => void;
    replyFn(write, reply);
    this.free(reply, replySize);
  }

  private installTrampolines(): void {
    this.writePtyFunctionIndex = this.addToTable(4, (_terminal, userdata, pointer, length) => {
      const writer = this.ptyWriters.get(userdata);
      if (!writer || length === 0) return;
      writer(this.memoryBytes().slice(pointer, pointer + length));
    });
    this.bellFunctionIndex = this.addToTable(2, (_terminal, userdata) => this.bells.get(userdata)?.());
    this.clipboardFunctionIndex = this.addToTable(3, (_terminal, userdata, write) =>
      this.clipboardWrite(userdata, write),
    );
  }

  /** Puts a trampoline to `callback` in the library's function table, and
   *  returns its index. */
  private addToTable(params: number, callback: (...args: number[]) => void): number {
    // small enough to compile at once, which the main thread allows
    let module = trampolines.get(params);
    if (!module) {
      module = new WebAssembly.Module(trampoline(params));
      trampolines.set(params, module);
    }
    const instance = new WebAssembly.Instance(module, { env: { callback } });
    const fn = instance.exports.callback;
    const table = this.exports.__indirect_function_table;
    if (typeof fn !== "function" || !(table instanceof WebAssembly.Table)) {
      throw new Error("libghostty-vt did not export its function table");
    }
    const index = table.length;
    // WebKit's grow(1, fn) keeps a broken type and traps on the call;
    // table.set gives the entry its type
    table.grow(1);
    table.set(index, fn);
    return index;
  }
}
