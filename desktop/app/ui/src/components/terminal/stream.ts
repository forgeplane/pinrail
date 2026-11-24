// A hosted terminal streamed into a TerminalView, over the core's attach
// WebSocket: a ticket, a snapshot, then the output from the snapshot's
// cut, with credit returned as the view parses it; the view's input and
// replies the other way. A stream whose connection drops reconnects and
// asks for only what it missed, or takes a fresh snapshot.

import type { InputKind, TerminalView } from "./view";

/** What a stream needs of a WebSocket, so tests can stand one in. */
export interface SocketLike {
  binaryType: string;
  readyState: number;
  send(data: string | ArrayBufferLike | ArrayBufferView): void;
  close(code?: number, reason?: string): void;
  onopen: ((event: unknown) => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: ((event: { code: number; reason?: string }) => void) | null;
  onerror: ((event: unknown) => void) | null;
}

export type Role = "observer" | "controller";
export type StreamState = "connecting" | "live" | "reconnecting" | "closed";

export interface ExitInfo {
  code?: number;
  signal?: number;
  reason?: string;
}

/** What the stream tells the pane. */
export interface StreamEvents {
  state?(state: StreamState, why?: string): void;
  role?(role: Role): void;
  /** The PTY's size, which an observer draws at. */
  size?(cols: number, rows: number): void;
  title?(title: string): void;
  exit?(exit: ExitInfo): void;
}

export interface StreamOptions {
  /** `<host>:<terminal id>` */
  terminal: string;
  view: TerminalView;
  /** The grid the view fits, sent with the attach. */
  cols: number;
  rows: number;
  /** Lines of history in a snapshot. */
  history?: number;
  events?: StreamEvents;
  /** A ticket for one attach. */
  ticket(terminal: string): Promise<string>;
  /** The attach address, without its query. */
  url(terminal: string): Promise<string>;
  /** Opens a socket; a real WebSocket unless a test gives another. */
  open?(url: string): SocketLike;
  /** Waits between reconnects; real timers unless a test gives another. */
  wait?(ms: number): Promise<void>;
}

const OUTPUT = 0x01;
const SNAPSHOT = 0x02;
const INPUT = 0x10;
const REPLY = 0x11;
/** The largest message the core takes from the window. */
const MAX_MESSAGE = 64 * 1024;
/** The waits between reconnects, then the last one again. */
const BACKOFF = [250, 1000, 2000, 5000, 10000];

/** Close codes after which a stream does not reconnect: the program
 *  exited, the terminal is gone, the origin was refused, or another
 *  attach replaced this one. */
const FINAL = new Set([1000, 4003, 4004, 4011]);

export class TerminalStream {
  private socket: SocketLike | null = null;
  private stopped = false;
  private attempts = 0;
  /** Messages are handled one after another: a snapshot is restored
   *  before the output that follows it is written. */
  private queue: Promise<void> = Promise.resolve();
  private snapshot: { format: string; chunks: Uint8Array[]; bytes: number } | null = null;
  /** The stream offset after the last output written, for a resume. */
  private offset: number | null = null;
  private credit = 0;
  private creditScheduled = false;
  private role: Role = "observer";
  private cols: number;
  private rows: number;
  private unsubscribe: (() => void)[] = [];

  constructor(private readonly options: StreamOptions) {
    this.cols = options.cols;
    this.rows = options.rows;
    const { view } = options;
    this.unsubscribe.push(
      view.onInput((bytes, kind) => this.input(bytes, kind)),
      view.onReply((bytes) => this.send(REPLY, bytes)),
    );
  }

  /** Connects, and keeps connecting until the stream ends. */
  start() {
    void this.connect();
  }

  /** Detaches; the terminal goes on running in the host. */
  close() {
    this.stopped = true;
    this.unsubscribe.forEach((u) => u());
    this.unsubscribe = [];
    this.socket?.close(1000, "detached");
    this.socket = null;
    this.setState("closed");
  }

  /** The grid the view now fits; the PTY takes it while this viewer
   *  controls the terminal. */
  resize(cols: number, rows: number) {
    if (cols === this.cols && rows === this.rows) return;
    this.cols = cols;
    this.rows = rows;
    this.text({ t: "resize", cols, rows });
  }

  /** Takes control without typing, or gives it up. */
  control(take: boolean) {
    this.text({ t: "control", take });
  }

  /** The view's colours, which the host answers the program's colour
   *  queries with while this viewer controls the terminal. */
  facts(facts: { fg: string; bg: string; cursor?: string; color_scheme: "dark" | "light" }) {
    this.text({ t: "facts", ...facts });
  }

  get currentRole(): Role {
    return this.role;
  }

  private setState(state: StreamState, why?: string) {
    this.options.events?.state?.(state, why);
  }

  private async connect() {
    if (this.stopped) return;
    this.setState(this.attempts === 0 ? "connecting" : "reconnecting");
    let url: string;
    try {
      const [ticket, base] = await Promise.all([
        this.options.ticket(this.options.terminal),
        this.options.url(this.options.terminal),
      ]);
      const query = new URLSearchParams({
        ticket,
        cols: String(this.cols),
        rows: String(this.rows),
        history: String(this.options.history ?? 10_000),
        formats: this.options.view.snapshotFormats.join(","),
      });
      if (this.offset !== null) query.set("resume_from", String(this.offset));
      url = `${base}?${query}`;
    } catch (error) {
      return this.retry(error instanceof Error ? error.message : String(error));
    }
    if (this.stopped) return;
    const open = this.options.open ?? ((u: string) => new WebSocket(u) as unknown as SocketLike);
    const socket = open(url);
    socket.binaryType = "arraybuffer";
    this.socket = socket;
    socket.onmessage = (event) => {
      const data = event.data;
      this.queue = this.queue.then(() => this.message(data)).catch(() => {});
    };
    // the close waits for the messages before it, so the exit is told
    // before the stream says it closed
    socket.onclose = (event) => {
      this.queue = this.queue.then(() => this.closed(socket, event));
    };
  }

  private closed(socket: SocketLike, event: { code: number; reason?: string }) {
    if (this.socket !== socket) return;
    this.socket = null;
    if (this.stopped) return;
    if (FINAL.has(event.code)) {
      this.stopped = true;
      this.setState("closed", event.reason);
      return;
    }
    void this.retry(event.reason || `closed with ${event.code}`);
  }

  private async retry(why: string) {
    if (this.stopped) return;
    const wait = BACKOFF[Math.min(this.attempts, BACKOFF.length - 1)];
    this.attempts++;
    this.setState("reconnecting", why);
    await (this.options.wait ?? ((ms) => new Promise((r) => setTimeout(r, ms))))(wait);
    await this.connect();
  }

  private async message(data: unknown) {
    if (typeof data === "string") return this.control_(JSON.parse(data) as Record<string, unknown>);
    const bytes = new Uint8Array(data as ArrayBuffer);
    if (bytes[0] === SNAPSHOT && this.snapshot) {
      const chunk = bytes.slice(1);
      this.snapshot.chunks.push(chunk);
      this.snapshot.bytes += chunk.length;
    } else if (bytes[0] === OUTPUT && bytes.length >= 9) {
      const at = Number(new DataView(bytes.buffer, bytes.byteOffset + 1, 8).getBigUint64(0, true));
      const output = bytes.slice(9);
      this.offset = at + output.length;
      // credit goes back on the connection that sent the bytes, not a later one
      const socket = this.socket;
      this.options.view.write(output, () => {
        if (this.socket === socket) this.returnCredit(output.length);
      });
    }
  }

  private async control_(message: Record<string, unknown>) {
    const events = this.options.events;
    switch (message.t) {
      case "hello":
        this.attempts = 0;
        this.setRole((message.role as Role) ?? "observer");
        if (typeof message.cols === "number" && typeof message.rows === "number") {
          events?.size?.(message.cols, message.rows);
        }
        this.setState("live");
        break;
      case "snapshot_begin":
        this.snapshot = { format: String(message.format), chunks: [], bytes: 0 };
        this.offset = typeof message.cut === "number" ? message.cut : this.offset;
        if (typeof message.cols === "number" && typeof message.rows === "number") {
          events?.size?.(message.cols, message.rows);
        }
        break;
      case "snapshot_end": {
        const snapshot = this.snapshot;
        this.snapshot = null;
        if (!snapshot) break;
        const whole = new Uint8Array(snapshot.bytes);
        let at = 0;
        for (const chunk of snapshot.chunks) {
          whole.set(chunk, at);
          at += chunk.length;
        }
        await this.options.view.restore(snapshot.format, whole);
        this.returnCredit(snapshot.bytes);
        break;
      }
      case "resume":
        if (typeof message.offset === "number") this.offset = message.offset;
        break;
      case "resync":
        // a snapshot follows; what the view shows is stale
        this.options.view.reset();
        break;
      case "role":
        this.setRole(message.role as Role);
        break;
      case "size":
        if (typeof message.cols === "number" && typeof message.rows === "number") {
          events?.size?.(message.cols, message.rows);
        }
        break;
      case "title":
        if (typeof message.text === "string") events?.title?.(message.text);
        break;
      case "exit":
        events?.exit?.({
          code: message.code as number | undefined,
          signal: message.signal as number | undefined,
          reason: message.reason as string | undefined,
        });
        break;
    }
  }

  private setRole(role: Role) {
    this.role = role;
    this.options.events?.role?.(role);
  }

  /** Credit for parsed bytes, sent once per task however many writes
   *  were parsed in it. */
  private returnCredit(bytes: number) {
    this.credit += bytes;
    if (this.creditScheduled) return;
    this.creditScheduled = true;
    queueMicrotask(() => {
      this.creditScheduled = false;
      const credit = this.credit;
      this.credit = 0;
      if (credit > 0) this.text({ t: "credit", bytes: credit });
    });
  }

  private input(bytes: Uint8Array, kind: InputKind) {
    // a focus report is the window's focus, which the core passes on only
    // for the controller; it never takes control
    if (kind === "focus") {
      const text = new TextDecoder().decode(bytes);
      if (text === "\x1b[I" || text === "\x1b[O") this.text({ t: "focus", in: text === "\x1b[I" });
      return;
    }
    this.send(INPUT, bytes);
  }

  private send(kind: number, bytes: Uint8Array) {
    // a large paste goes in pieces the core takes
    for (let at = 0; at < bytes.length || at === 0; at += MAX_MESSAGE - 1) {
      const piece = bytes.subarray(at, at + MAX_MESSAGE - 1);
      const message = new Uint8Array(piece.length + 1);
      message[0] = kind;
      message.set(piece, 1);
      this.raw(message);
      if (bytes.length === 0) break;
    }
  }

  private text(value: unknown) {
    this.raw(JSON.stringify(value));
  }

  private raw(data: string | Uint8Array) {
    const socket = this.socket;
    // 1 is OPEN; what is sent while connecting is dropped, as the view
    // is redrawn from the snapshot of the next connection
    if (socket && socket.readyState === 1) socket.send(data);
  }
}
