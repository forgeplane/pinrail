// The output a terminal is given, parsed a piece at a time: no animation
// frame spends more than 6 ms parsing, so a burst cannot hold the window,
// and each write's bytes are credited back once they are parsed, as the
// core's flow control expects. While the program holds
// its output in synchronized mode (2026), drawing waits for it, for at most
// 150 ms. A changed title or working folder is reported once, read after
// each run of parsing and only when someone listens for it. The echo
// of what the person typed is parsed and drawn at once, without waiting
// for a frame, when nothing else waits. WebKit gives a covered or
// minimized window no frames, so output also parses on a timer when no
// frame comes, and the core keeps receiving credit.

/** What the queue needs of the terminal it feeds. */
export interface Parser {
  write(bytes: Uint8Array): void;
  isSynchronizedOutput(): boolean;
  title(): string;
  pwd(): string;
}

/** What the queue tells the view. */
export interface QueueEvents {
  /** The screen changed and can be drawn. */
  draw(): void;
  /** Reports a new title; the queue reads the title only while this is
   *  set, which may change. */
  title?(title: string): void;
  pwd?(pwd: string): void;
}

/** How the queue waits for the next frame, and tells the time; a test
 *  gives its own. */
export interface Clock {
  /** Runs `run` on the next frame, unless the returned function cancels it. */
  frame(run: () => void): () => void;
  /** Runs `run` after `ms`, unless the returned function cancels it. */
  later(run: () => void, ms: number): () => void;
  now(): number;
}

/** The most bytes parsed at once. */
export const PIECE = 32 * 1024;
/** The most time spent parsing in one frame. */
export const FRAME_BUDGET_MS = 6;
/** How long the queue waits for a frame before it parses without one. */
export const FRAME_WAIT_MS = 50;
/** The longest drawing waits for synchronized output to end. */
export const SYNC_LIMIT_MS = 150;
/** How long after a key its echo is drawn at once. */
export const ECHO_MS = 250;
/** How much output after a key is drawn at once, at most. */
export const ECHO_BYTES = 16 * 1024;

const browserClock: Clock = {
  frame: (run) => {
    const id = requestAnimationFrame(() => run());
    return () => cancelAnimationFrame(id);
  },
  later: (run, ms) => {
    const id = setTimeout(run, ms);
    return () => clearTimeout(id);
  },
  now: () => performance.now(),
};

type Pending = { bytes: Uint8Array; at: number; consumed: () => void };

export class WriteQueue {
  /** The writes waiting, from `first` on: the parsed ones before it are
   *  dropped in batches, rather than one shift at a time. */
  private pending: Pending[] = [];
  private first = 0;
  /** Cancels the pending frame and timer, whichever has not run; null
   *  when none waits. */
  private scheduled: (() => void) | null = null;
  /** When the program began holding its output, if it does. */
  private syncSince: number | null = null;
  private syncTimer = false;
  private lastTitle = "";
  private lastPwd = "";
  private stopped = false;
  /** Until when, and how many more bytes, output is parsed at once. */
  private echoUntil = -Infinity;
  private echoBytes = 0;

  constructor(
    private readonly parser: Parser,
    private readonly events: QueueEvents,
    private readonly clock: Clock = browserClock,
  ) {}

  /** Queues output; `consumed` is called once all of it is parsed. */
  write(bytes: Uint8Array, consumed: () => void): void {
    if (this.stopped) return;
    if (bytes.length === 0) {
      consumed();
      return;
    }
    if (this.isEcho(bytes)) {
      this.echoBytes -= bytes.length;
      this.parser.write(bytes);
      consumed();
      this.report();
      this.drawWhenReady();
      return;
    }
    this.pending.push({ bytes, at: 0, consumed });
    this.schedule();
  }

  /** How many writes wait. */
  private get count(): number {
    return this.pending.length - this.first;
  }

  /** The person typed: the output that follows soon after, a little of
   *  it, is likely the echo, and is parsed and drawn at once. */
  typed(): void {
    this.echoUntil = this.clock.now() + ECHO_MS;
    this.echoBytes = ECHO_BYTES;
  }

  /** Output to parse at once: soon after a key, within the echo's bytes,
   *  and with nothing waiting before it. */
  private isEcho(bytes: Uint8Array): boolean {
    return this.count === 0 && this.clock.now() < this.echoUntil && bytes.length <= this.echoBytes;
  }

  /** Bytes still waiting to be parsed. */
  get waiting(): number {
    let n = 0;
    for (let i = this.first; i < this.pending.length; i++) n += this.pending[i]!.bytes.length - this.pending[i]!.at;
    return n;
  }

  /** Drops what waits. With `credit`, as when a snapshot replaces the
   *  screen on a live connection, its credit is returned, or the host
   *  would wait for it for good; without, the stream it came from is
   *  gone. */
  stop({ credit = false }: { credit?: boolean } = {}): void {
    this.stopped = true;
    this.scheduled?.();
    this.scheduled = null;
    const dropped = this.pending.slice(this.first);
    this.pending = [];
    this.first = 0;
    if (credit) for (const p of dropped) p.consumed();
  }

  /** Parses on the next frame, or after `FRAME_WAIT_MS` when no frame
   *  comes; the other is cancelled. */
  private schedule() {
    if (this.scheduled || this.stopped) return;
    let cancelFrame = () => {};
    let cancelTimer = () => {};
    const go = (other: () => void) => {
      other();
      this.scheduled = null;
      this.run();
    };
    cancelFrame = this.clock.frame(() => go(cancelTimer));
    cancelTimer = this.clock.later(() => go(cancelFrame), FRAME_WAIT_MS);
    this.scheduled = () => {
      cancelFrame();
      cancelTimer();
    };
  }

  private run() {
    if (this.stopped) return;
    const started = this.clock.now();
    let parsed = false;
    while (this.count && this.clock.now() - started < FRAME_BUDGET_MS) {
      const head = this.pending[this.first]!;
      const end = Math.min(head.bytes.length, head.at + PIECE);
      this.parser.write(head.bytes.subarray(head.at, end));
      head.at = end;
      parsed = true;
      if (head.at === head.bytes.length) {
        this.first++;
        head.consumed();
      }
    }
    if (this.count === 0) {
      this.pending = [];
      this.first = 0;
    } else if (this.first > 64) {
      this.pending = this.pending.slice(this.first);
      this.first = 0;
    }
    if (parsed) this.report();
    if (this.count) this.schedule();
    if (parsed) this.drawWhenReady();
  }

  /** A changed title or working folder, each read only when someone
   *  listens for it. */
  private report() {
    if (this.events.title) {
      const title = this.parser.title();
      if (title !== this.lastTitle) {
        this.lastTitle = title;
        this.events.title(title);
      }
    }
    if (this.events.pwd) {
      const pwd = this.parser.pwd();
      if (pwd !== this.lastPwd) {
        this.lastPwd = pwd;
        this.events.pwd(pwd);
      }
    }
  }

  /** Draws, unless the program holds its output, for at most 150 ms. */
  private drawWhenReady() {
    if (!this.parser.isSynchronizedOutput()) {
      this.syncSince = null;
      this.events.draw();
      return;
    }
    const now = this.clock.now();
    this.syncSince ??= now;
    if (now - this.syncSince >= SYNC_LIMIT_MS) {
      this.events.draw();
      return;
    }
    // drawn when the hold ends with more output, or at the limit
    if (this.syncTimer) return;
    this.syncTimer = true;
    this.clock.later(
      () => {
        this.syncTimer = false;
        if (!this.stopped) this.drawWhenReady();
      },
      SYNC_LIMIT_MS - (now - this.syncSince),
    );
  }
}
