// The output a terminal is given, parsed a piece at a time: no animation
// frame spends more than 6 ms parsing, so a burst cannot hold the window,
// and each write's bytes are credited back once they are parsed, as the
// core's flow control expects (runtime spec 8.3). While the program holds
// its output in synchronized mode (2026), drawing waits for it, for at most
// 150 ms. A changed title or working folder is reported once. The echo
// of what the person typed is parsed and drawn at once, without waiting
// for a frame, when nothing else waits.

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
  title?(title: string): void;
  pwd?(pwd: string): void;
}

/** How the queue waits for the next frame, and tells the time; a test
 *  gives its own. */
export interface Clock {
  frame(run: () => void): void;
  later(run: () => void, ms: number): void;
  now(): number;
}

/** The most bytes parsed at once. */
export const PIECE = 32 * 1024;
/** The most time spent parsing in one frame. */
export const FRAME_BUDGET_MS = 6;
/** The longest drawing waits for synchronized output to end. */
export const SYNC_LIMIT_MS = 150;
/** How long after a key its echo is drawn at once. */
export const ECHO_MS = 250;
/** How much output after a key is drawn at once, at most. */
export const ECHO_BYTES = 16 * 1024;

const browserClock: Clock = {
  frame: (run) => requestAnimationFrame(() => run()),
  later: (run, ms) => setTimeout(run, ms),
  now: () => performance.now(),
};

type Pending = { bytes: Uint8Array; at: number; consumed: () => void };

export class WriteQueue {
  private pending: Pending[] = [];
  private scheduled = false;
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

  /** The person typed: the output that follows soon after, a little of
   *  it, is likely the echo, and is parsed and drawn at once. */
  typed(): void {
    this.echoUntil = this.clock.now() + ECHO_MS;
    this.echoBytes = ECHO_BYTES;
  }

  /** Output to parse at once: soon after a key, within the echo's bytes,
   *  and with nothing waiting before it. */
  private isEcho(bytes: Uint8Array): boolean {
    return this.pending.length === 0 && this.clock.now() < this.echoUntil && bytes.length <= this.echoBytes;
  }

  /** Bytes still waiting to be parsed. */
  get waiting(): number {
    return this.pending.reduce((n, p) => n + p.bytes.length - p.at, 0);
  }

  /** Drops what waits; its credit is not returned, as the stream it came
   *  from is gone. */
  stop(): void {
    this.stopped = true;
    this.pending = [];
  }

  private schedule() {
    if (this.scheduled || this.stopped) return;
    this.scheduled = true;
    this.clock.frame(() => {
      this.scheduled = false;
      this.run();
    });
  }

  private run() {
    if (this.stopped) return;
    const started = this.clock.now();
    let parsed = false;
    while (this.pending.length && this.clock.now() - started < FRAME_BUDGET_MS) {
      const head = this.pending[0];
      const end = Math.min(head.bytes.length, head.at + PIECE);
      this.parser.write(head.bytes.subarray(head.at, end));
      head.at = end;
      parsed = true;
      if (head.at === head.bytes.length) {
        this.pending.shift();
        head.consumed();
      }
      this.report();
    }
    if (this.pending.length) this.schedule();
    if (parsed) this.drawWhenReady();
  }

  /** A changed title or working folder. */
  private report() {
    const title = this.parser.title();
    if (title !== this.lastTitle) {
      this.lastTitle = title;
      this.events.title?.(title);
    }
    const pwd = this.parser.pwd();
    if (pwd !== this.lastPwd) {
      this.lastPwd = pwd;
      this.events.pwd?.(pwd);
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
