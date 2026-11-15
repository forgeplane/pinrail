import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { Mirror, type MirrorSink, readableText } from "./mirror";
import type { GhosttyRow } from "./types";

const row = (text: string) => ({ text, cells: [], isWrapContinuation: false, wrapsToNext: false }) as GhosttyRow;

function rig() {
  const calls: string[] = [];
  const sink: MirrorSink = {
    setRowCount: (n) => calls.push(`count ${n}`),
    setRow: (i, t) => calls.push(`row ${i} ${t}`),
    announce: (t) => calls.push(`say ${t}`),
  };
  return { mirror: new Mirror(sink), calls };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("the rows", () => {
  test("are updated at once, then at most every 100 ms with the latest", () => {
    const { mirror, calls } = rig();
    mirror.update([row("a"), row("b")]);
    expect(calls).toEqual(["count 2", "row 0 a", "row 1 b"]);
    calls.length = 0;
    mirror.update([row("a"), row("c")]);
    mirror.update([row("a"), row("d")]);
    vi.advanceTimersByTime(99);
    expect(calls).toEqual([]);
    vi.advanceTimersByTime(1);
    // only the row that changed, and only its latest text
    expect(calls).toEqual(["row 1 d"]);
  });

  test("follow the row count, and ignore trailing blanks", () => {
    const { mirror, calls } = rig();
    mirror.update([row("a   ")]);
    vi.advanceTimersByTime(100);
    mirror.update([row("a"), row("b")]);
    expect(calls).toEqual(["count 1", "row 0 a", "count 2", "row 1 b"]);
  });
});

describe("announcements", () => {
  test("batch output over 250 ms", () => {
    const { mirror, calls } = rig();
    mirror.output("Compiling ");
    vi.advanceTimersByTime(200);
    mirror.output("done\r\n");
    expect(calls).toEqual([]);
    vi.advanceTimersByTime(50);
    expect(calls).toEqual(["say Compiling done"]);
  });

  test("keep the last 500 characters of a batch", () => {
    const { mirror, calls } = rig();
    mirror.output("x".repeat(400) + "y".repeat(400));
    vi.advanceTimersByTime(250);
    expect(calls).toEqual([`say ${"x".repeat(100)}${"y".repeat(400)}`]);
  });

  test("leave out the echo of typed characters", () => {
    const { mirror, calls } = rig();
    mirror.typed("ls");
    mirror.output("l");
    mirror.output("s");
    vi.advanceTimersByTime(250);
    expect(calls).toEqual([]);
    mirror.typed("\r");
    mirror.output("\r\nfile.txt\r\n");
    vi.advanceTimersByTime(250);
    expect(calls).toEqual(["say file.txt"]);
  });
});

test("readable text has no escape sequences or control characters", () => {
  expect(readableText("\x1b[1;31mred\x1b[0m \x1b]0;title\x07ok\x1b]8;;https://a.io\x1b\\link\x1b(B\x07\r\n")).toBe(
    "red oklink\n",
  );
});
