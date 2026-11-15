import { describe, expect, test } from "vitest";
import { regressions, summarise } from "./baseline.mjs";

/** The results of one run: a burst, an input case and an idle case. */
const run = ({ burst = 200, frame = 20, input = 4, idle = 0 } = {}) => [
  {
    runner: "webkit",
    adapter: "ghostty",
    case: "burst",
    item: "synthetic/tui.bin",
    totalMs: burst,
    frames: { max: frame },
  },
  { runner: "webkit", adapter: "ghostty", case: "input", latency: { p95: input } },
  { runner: "webkit", adapter: "ghostty", case: "idle", drawnPerSecond: idle },
  { runner: "webkit", adapter: "xterm", case: "burst", item: "synthetic/tui.bin", error: "timed out" },
];

describe("a baseline", () => {
  test("holds each renderer's measures, without the cases that failed", () => {
    expect(summarise(run())).toEqual({
      "webkit/ghostty": {
        burst_tui_ms: 200,
        burst_tui_longest_frame_ms: 20,
        input_p95_ms: 4,
        idle_frames_per_s: 0,
      },
      "webkit/xterm": {},
    });
  });
});

describe("regressions", () => {
  const baseline = summarise(run());

  test("are results worse by more than 15 % and the margin", () => {
    // 200 ms × 1.15 + 50 ms = 280 ms
    expect(regressions(summarise(run({ burst: 280 })), baseline)).toEqual([]);
    expect(regressions(summarise(run({ burst: 281 })), baseline)).toEqual([
      "webkit/ghostty burst_tui_ms: 281 ms is worse than the baseline's 200 ms by more than 15 % and 50 ms",
    ]);
  });

  test("leave small values their margin, where a ratio means little", () => {
    // 4 ms to 9 ms is more than twice, but within 4 × 1.15 + 5 ms
    expect(regressions(summarise(run({ input: 9 })), baseline)).toEqual([]);
  });

  test("count any frame drawn when idle, where the baseline drew none", () => {
    expect(regressions(summarise(run({ idle: 1 })), baseline)).toHaveLength(1);
  });

  test("ignore measures the baseline does not have, and improvements", () => {
    expect(regressions(summarise(run({ burst: 100 })), {})).toEqual([]);
    expect(regressions(summarise(run({ burst: 100, frame: 5 })), baseline)).toEqual([]);
  });
});
