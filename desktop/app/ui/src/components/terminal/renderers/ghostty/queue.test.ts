import { describe, expect, test } from "vitest";
import {
  ECHO_BYTES,
  ECHO_MS,
  FRAME_BUDGET_MS,
  FRAME_WAIT_MS,
  PIECE,
  SYNC_LIMIT_MS,
  WriteQueue,
  type Clock,
  type Parser,
} from "./queue";

/** A clock the test moves: frames run when it says, and parsing a piece
 *  takes `perPiece` ms. */
function rig({ perPiece = 1, sync = () => false }: { perPiece?: number; sync?: () => boolean } = {}) {
  let time = 0;
  const frames: (() => void)[] = [];
  const timers: { at: number; run: () => void }[] = [];
  const clock: Clock = {
    frame: (run) => frames.push(run),
    later: (run, ms) => timers.push({ at: time + ms, run }),
    now: () => time,
  };
  const pieces: number[] = [];
  let title = "";
  let pwd = "";
  const parser: Parser = {
    write: (bytes) => {
      pieces.push(bytes.length);
      time += perPiece;
    },
    isSynchronizedOutput: sync,
    title: () => title,
    pwd: () => pwd,
  };
  const seen: string[] = [];
  const queue = new WriteQueue(
    parser,
    { draw: () => seen.push("draw"), title: (t) => seen.push(`title:${t}`), pwd: (p) => seen.push(`pwd:${p}`) },
    clock,
  );
  return {
    queue,
    pieces,
    seen,
    setTitle: (t: string) => (title = t),
    setPwd: (p: string) => (pwd = p),
    /** Runs the next frame; false when none is waiting. */
    frame: () => {
      const next = frames.shift();
      next?.();
      return !!next;
    },
    /** Moves time on, running the timers that fall due. */
    advance: (ms: number) => {
      time += ms;
      const due = timers.filter((t) => t.at <= time);
      timers.splice(0, timers.length, ...timers.filter((t) => t.at > time));
      for (const t of due) t.run();
    },
  };
}

describe("the write queue", () => {
  test("parses in pieces of 32 KiB, and credits a write once it is all parsed", () => {
    const { queue, pieces, frame } = rig();
    const credited: number[] = [];
    queue.write(new Uint8Array(PIECE * 2 + 10), () => credited.push(1));
    queue.write(new Uint8Array(5), () => credited.push(2));
    expect(pieces).toEqual([]);
    frame();
    expect(pieces).toEqual([PIECE, PIECE, 10, 5]);
    expect(credited).toEqual([1, 2]);
  });

  test("spends no more than 6 ms of a frame parsing, and goes on in the next", () => {
    const { queue, pieces, frame } = rig({ perPiece: 4 });
    let credited = false;
    queue.write(new Uint8Array(PIECE * 5), () => (credited = true));
    frame();
    // 4 ms, then 8 ms: the second piece ends past the budget, and parsing stops
    expect(pieces.length).toBe(Math.ceil(FRAME_BUDGET_MS / 4));
    expect(credited).toBe(false);
    expect(queue.waiting).toBe(PIECE * 3);
    while (frame());
    expect(pieces.length).toBe(5);
    expect(credited).toBe(true);
  });

  test("draws after it parses, and reports a new title or folder once", () => {
    const { queue, seen, frame, setTitle, setPwd } = rig();
    setTitle("vim");
    queue.write(new Uint8Array(3), () => {});
    frame();
    queue.write(new Uint8Array(3), () => {});
    setPwd("file://host/work");
    frame();
    expect(seen).toEqual(["title:vim", "draw", "pwd:file://host/work", "draw"]);
  });

  test("holds drawing while the program holds its output, for at most 150 ms", () => {
    let holding = true;
    const { queue, seen, frame, advance } = rig({ sync: () => holding });
    queue.write(new Uint8Array(3), () => {});
    frame();
    expect(seen).toEqual([]);
    // the program ends the hold with more output: drawn at once
    holding = false;
    queue.write(new Uint8Array(3), () => {});
    frame();
    expect(seen).toEqual(["draw"]);

    // a hold that never ends is drawn at the limit
    holding = true;
    queue.write(new Uint8Array(3), () => {});
    frame();
    advance(SYNC_LIMIT_MS - 1);
    expect(seen).toEqual(["draw"]);
    advance(1);
    expect(seen).toEqual(["draw", "draw"]);
  });

  test("credits an empty write at once, and drops what waits once stopped", () => {
    const { queue, pieces, frame } = rig();
    let empty = false;
    queue.write(new Uint8Array(0), () => (empty = true));
    expect(empty).toBe(true);
    let late = false;
    queue.write(new Uint8Array(4), () => (late = true));
    queue.stop();
    frame();
    expect(pieces).toEqual([]);
    expect(late).toBe(false);
  });
});

describe("a screen replaced while output waits", () => {
  test("credits what it drops, as the connection goes on", () => {
    const { queue, pieces, frame } = rig();
    const credited: string[] = [];
    queue.write(new Uint8Array(PIECE * 10), () => credited.push("partly parsed"));
    queue.write(new Uint8Array(4), () => credited.push("waiting"));
    // too much for one frame's budget: the first write is only begun
    frame();
    expect(pieces.length).toBeGreaterThan(0);
    queue.stop({ credit: true });
    expect(credited).toEqual(["partly parsed", "waiting"]);
  });
});

describe("the echo of what the person typed", () => {
  test("is parsed and drawn at once, without waiting for a frame", () => {
    const { queue, pieces, seen } = rig();
    const credited: number[] = [];
    queue.typed();
    queue.write(new Uint8Array(1), () => credited.push(1));
    expect(pieces).toEqual([1]);
    expect(credited).toEqual([1]);
    expect(seen).toEqual(["draw"]);
  });

  test("waits for a frame when nothing was typed", () => {
    const { queue, pieces, frame } = rig();
    queue.write(new Uint8Array(1), () => {});
    expect(pieces).toEqual([]);
    frame();
    expect(pieces).toEqual([1]);
  });

  test("waits behind output already queued, so nothing is reordered", () => {
    const { queue, pieces, frame } = rig();
    queue.write(new Uint8Array(100), () => {});
    queue.typed();
    queue.write(new Uint8Array(1), () => {});
    expect(pieces).toEqual([]);
    frame();
    expect(pieces).toEqual([100, 1]);
  });

  test("covers only a little output, soon after the key", () => {
    const { queue, pieces, frame, advance } = rig();
    queue.typed();
    queue.write(new Uint8Array(ECHO_BYTES), () => {});
    // the budget is spent: more output waits for a frame
    queue.write(new Uint8Array(1), () => {});
    expect(pieces).toEqual([ECHO_BYTES]);
    frame();
    queue.typed();
    advance(ECHO_MS);
    queue.write(new Uint8Array(1), () => {});
    expect(pieces).toEqual([ECHO_BYTES, 1]);
    frame();
    expect(pieces).toEqual([ECHO_BYTES, 1, 1]);
  });
});

describe("without animation frames", () => {
  test("parses output and returns its credit when no frame comes", () => {
    const { queue, pieces, advance } = rig();
    let consumed = false;
    queue.write(new Uint8Array(100), () => (consumed = true));
    advance(FRAME_WAIT_MS);
    expect(pieces).toEqual([100]);
    expect(consumed).toBe(true);
  });

  test("parses a long write a piece at a time, a budget per wait", () => {
    const { queue, pieces, advance } = rig({ perPiece: FRAME_BUDGET_MS });
    queue.write(new Uint8Array(PIECE * 2), () => {});
    advance(FRAME_WAIT_MS);
    expect(pieces).toEqual([PIECE]);
    advance(FRAME_WAIT_MS);
    expect(pieces).toEqual([PIECE, PIECE]);
  });

  test("parses once when the frame comes after all", () => {
    const { queue, pieces, frame, advance } = rig();
    queue.write(new Uint8Array(100), () => {});
    frame();
    advance(FRAME_WAIT_MS);
    expect(pieces).toEqual([100]);
  });
});
