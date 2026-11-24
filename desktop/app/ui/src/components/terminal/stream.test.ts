import { describe, expect, test } from "vitest";
import { TerminalStream, type SocketLike, type StreamEvents } from "./stream";
import type { InputKind, TerminalView } from "./view";

const bytes = (s: string) => new TextEncoder().encode(s);
const text = (b: Uint8Array) => new TextDecoder().decode(b);
const flush = () => new Promise((r) => setTimeout(r, 0));

class FakeSocket implements SocketLike {
  binaryType = "";
  readyState = 1;
  sent: (string | Uint8Array)[] = [];
  closed: number | null = null;
  onopen = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number; reason?: string }) => void) | null = null;
  onerror = null;
  constructor(readonly url: string) {}
  send(data: string | ArrayBufferLike | ArrayBufferView) {
    this.sent.push(typeof data === "string" ? data : new Uint8Array(data as ArrayBuffer));
  }
  close(code = 1000) {
    this.closed = code;
  }
  json(value: unknown) {
    this.onmessage?.({ data: JSON.stringify(value) });
  }
  binary(kind: number, payload: Uint8Array, offset?: number) {
    const head = offset === undefined ? 1 : 9;
    const message = new Uint8Array(head + payload.length);
    message[0] = kind;
    if (offset !== undefined) new DataView(message.buffer).setBigUint64(1, BigInt(offset), true);
    message.set(payload, head);
    this.onmessage?.({ data: message.buffer });
  }
  drop(code: number, reason = "") {
    this.readyState = 3;
    this.onclose?.({ code, reason });
  }
  controls() {
    return this.sent.filter((m): m is string => typeof m === "string").map((m) => JSON.parse(m));
  }
  binaries() {
    return this.sent.filter((m): m is Uint8Array => typeof m !== "string");
  }
}

class FakeView {
  snapshotFormats = ["vt"] as const;
  written: string[] = [];
  restored: [string, string][] = [];
  resets = 0;
  /** writes wait here until the test lets them be parsed */
  pending: (() => void)[] = [];
  input?: (bytes: Uint8Array, kind: InputKind) => void;
  reply?: (bytes: Uint8Array) => void;
  write(b: Uint8Array, consumed: () => void) {
    this.written.push(text(b));
    this.pending.push(consumed);
  }
  parse() {
    this.pending.splice(0).forEach((c) => c());
  }
  async restore(format: string, b: Uint8Array) {
    this.restored.push([format, text(b)]);
  }
  reset() {
    this.resets++;
  }
  onInput(l: (bytes: Uint8Array, kind: InputKind) => void) {
    this.input = l;
    return () => (this.input = undefined);
  }
  onReply(l: (bytes: Uint8Array) => void) {
    this.reply = l;
    return () => (this.reply = undefined);
  }
}

function setup(events: StreamEvents = {}) {
  const view = new FakeView();
  const sockets: FakeSocket[] = [];
  const waits: number[] = [];
  let tickets = 0;
  const stream = new TerminalStream({
    terminal: "local:t1",
    view: view as unknown as TerminalView,
    cols: 100,
    rows: 30,
    events,
    ticket: async () => `ticket-${++tickets}`,
    url: async () => "ws://core/api/v1/terminals/local%3At1/attach",
    open: (url) => {
      const socket = new FakeSocket(url);
      sockets.push(socket);
      return socket;
    },
    wait: async (ms) => {
      waits.push(ms);
    },
  });
  return { view, sockets, waits, stream };
}

/** A first attach: hello, a snapshot cut at `cut`, and its end. */
async function attach(socket: FakeSocket, cut = 0, screen = "screen") {
  socket.json({ t: "hello", v: 1, role: "observer", cols: 80, rows: 24 });
  socket.json({ t: "snapshot_begin", format: "vt", cols: 80, rows: 24, cut });
  socket.binary(0x02, bytes(screen));
  socket.json({ t: "snapshot_end" });
  await flush();
}

describe("TerminalStream", () => {
  test("attaches with a ticket, the view's grid and its snapshot formats", async () => {
    const { sockets, stream } = setup();
    stream.start();
    await flush();
    const url = new URL(sockets[0].url);
    expect(Object.fromEntries(url.searchParams)).toEqual({
      ticket: "ticket-1",
      cols: "100",
      rows: "30",
      history: "10000",
      formats: "vt",
    });
    expect(sockets[0].binaryType).toBe("arraybuffer");
  });

  test("restores the snapshot before writing the output that follows it", async () => {
    const { view, sockets, stream } = setup();
    stream.start();
    await flush();
    const socket = sockets[0];
    socket.json({ t: "hello", v: 1, role: "observer", cols: 80, rows: 24 });
    socket.json({ t: "snapshot_begin", format: "vt", cols: 80, rows: 24, cut: 10 });
    socket.binary(0x02, bytes("scr"));
    socket.binary(0x02, bytes("een"));
    socket.json({ t: "snapshot_end" });
    socket.binary(0x01, bytes("after"), 10);
    await flush();
    expect(view.restored).toEqual([["vt", "screen"]]);
    expect(view.written).toEqual(["after"]);
  });

  test("returns credit once the view has parsed the bytes, gathered per task", async () => {
    const { view, sockets, stream } = setup();
    stream.start();
    await flush();
    const socket = sockets[0];
    await attach(socket, 0, "abc");
    socket.binary(0x01, bytes("12345"), 0);
    socket.binary(0x01, bytes("678"), 5);
    await flush();
    // the snapshot's bytes come back once it is restored
    expect(socket.controls().filter((c) => c.t === "credit")).toEqual([{ t: "credit", bytes: 3 }]);
    view.parse();
    await flush();
    expect(socket.controls().filter((c) => c.t === "credit")).toEqual([
      { t: "credit", bytes: 3 },
      { t: "credit", bytes: 8 },
    ]);
  });

  test("returns no credit on a new connection for output the old one sent", async () => {
    const { view, sockets, stream } = setup();
    stream.start();
    await flush();
    await attach(sockets[0], 100);
    sockets[0].binary(0x01, bytes("hello"), 100);
    await flush();
    sockets[0].drop(4010, "host offline");
    await flush();
    await attach(sockets[1], 105);
    // the view drops the old output, and credits it, once replaced
    view.parse();
    await flush();
    // the new snapshot's bytes only
    expect(sockets[1].controls().filter((c) => c.t === "credit")).toEqual([{ t: "credit", bytes: 6 }]);
  });

  test("sends the person's input and the emulator's replies as their own kinds", async () => {
    const { view, sockets, stream } = setup();
    stream.start();
    await flush();
    await attach(sockets[0]);
    view.input!(bytes("ls\r"), "key");
    view.reply!(bytes("\x1b[1;1R"));
    view.input!(bytes("\x1b[I"), "focus");
    const binaries = sockets[0].binaries();
    expect(binaries.map((b) => [b[0], text(b.subarray(1))])).toEqual([
      [0x10, "ls\r"],
      [0x11, "\x1b[1;1R"],
    ]);
    // focus is the window's, never input that would take control
    expect(sockets[0].controls().filter((c) => c.t === "focus")).toEqual([{ t: "focus", in: true }]);
  });

  test("sends a paste larger than a message in pieces the core takes", async () => {
    const { view, sockets, stream } = setup();
    stream.start();
    await flush();
    await attach(sockets[0]);
    view.input!(new Uint8Array(150 * 1024).fill(0x61), "paste");
    const pieces = sockets[0].binaries();
    expect(pieces.every((p) => p.length <= 64 * 1024 && p[0] === 0x10)).toBe(true);
    expect(pieces.reduce((n, p) => n + p.length - 1, 0)).toBe(150 * 1024);
  });

  test("reports the role, the PTY's size, the title and the exit", async () => {
    const seen: unknown[] = [];
    const { sockets, stream } = setup({
      role: (r) => seen.push(["role", r]),
      size: (c, r) => seen.push(["size", c, r]),
      title: (t) => seen.push(["title", t]),
      exit: (e) => seen.push(["exit", e]),
      state: (s) => seen.push(["state", s]),
    });
    stream.start();
    await flush();
    const socket = sockets[0];
    await attach(socket);
    socket.json({ t: "role", role: "controller" });
    socket.json({ t: "title", text: "vim" });
    socket.json({ t: "exit", reason: "exited", code: 0 });
    socket.drop(1000, "exited");
    await flush();
    expect(seen).toEqual([
      ["state", "connecting"],
      ["role", "observer"],
      ["size", 80, 24],
      ["state", "live"],
      ["size", 80, 24],
      ["role", "controller"],
      ["title", "vim"],
      ["exit", { reason: "exited", code: 0, signal: undefined }],
      ["state", "closed"],
    ]);
    expect(stream.currentRole).toBe("controller");
  });

  test("reconnects after a drop and asks for the output from where it stopped", async () => {
    const { sockets, waits, stream } = setup();
    stream.start();
    await flush();
    await attach(sockets[0], 100);
    sockets[0].binary(0x01, bytes("hello"), 100);
    await flush();
    sockets[0].drop(4010, "host offline");
    await flush();
    expect(waits).toEqual([250]);
    const url = new URL(sockets[1].url);
    expect(url.searchParams.get("resume_from")).toBe("105");
    expect(url.searchParams.get("ticket")).toBe("ticket-2");
  });

  test("backs off while reconnecting fails, and starts again after a hello", async () => {
    const { sockets, waits, stream } = setup();
    stream.start();
    await flush();
    for (let i = 0; i < 6; i++) {
      sockets[i].drop(4010);
      await flush();
    }
    expect(waits).toEqual([250, 1000, 2000, 5000, 10000, 10000]);
    await attach(sockets[6]);
    sockets[6].drop(4008, "too slow");
    await flush();
    expect(waits.at(-1)).toBe(250);
  });

  test("does not reconnect when the program exited, the terminal is gone or another attach replaced it", async () => {
    for (const code of [1000, 4004, 4011]) {
      const { sockets, stream } = setup();
      stream.start();
      await flush();
      sockets[0].drop(code);
      await flush();
      expect(sockets).toHaveLength(1);
    }
  });

  test("a resync resets the view before the snapshot that follows", async () => {
    const { view, sockets, stream } = setup();
    stream.start();
    await flush();
    await attach(sockets[0]);
    sockets[0].json({ t: "resync" });
    sockets[0].json({ t: "snapshot_begin", format: "vt", cols: 80, rows: 24, cut: 50 });
    sockets[0].binary(0x02, bytes("fresh"));
    sockets[0].json({ t: "snapshot_end" });
    await flush();
    expect(view.resets).toBe(1);
    expect(view.restored.at(-1)).toEqual(["vt", "fresh"]);
  });

  test("closing detaches the view and stops reconnecting", async () => {
    const { view, sockets, stream } = setup();
    stream.start();
    await flush();
    await attach(sockets[0]);
    stream.close();
    expect(sockets[0].closed).toBe(1000);
    expect(view.input).toBeUndefined();
    sockets[0].drop(1006);
    await flush();
    expect(sockets).toHaveLength(1);
  });

  test("sends a resize only when the grid changes", async () => {
    const { sockets, stream } = setup();
    stream.start();
    await flush();
    await attach(sockets[0]);
    stream.resize(100, 30);
    stream.resize(120, 40);
    expect(sockets[0].controls().filter((c) => c.t === "resize")).toEqual([{ t: "resize", cols: 120, rows: 40 }]);
  });
});
