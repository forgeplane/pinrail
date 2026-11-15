import { describe, expect, test } from "vitest";
import type { Run } from "../../api/runtime";
import { lasted, runLineText } from "./runLines";

const run = (over: Partial<Run>): Run => ({
  id: "run_1",
  host: "local",
  agent: "claude-code",
  kind: "start",
  state: "ended",
  terminal: null,
  folder: "/work",
  started_at: "2026-11-14T10:00:00Z",
  ended_at: "2026-11-14T10:30:00Z",
  end_reason: "exited",
  exit_code: 0,
  exit_signal: null,
  session: null,
  ...over,
});

describe("the run lines", () => {
  test("say where a run started, and how it ended", () => {
    expect(runLineText(run({}), false)).toBe("Started in Pinrail");
    expect(runLineText(run({ kind: "resume" }), false)).toBe("Resumed in Pinrail");
    expect(runLineText(run({ exit_code: 1 }), true)).toBe("Exited with code 1");
    expect(runLineText(run({ end_reason: "stopped" }), true)).toBe("Stopped by you");
    expect(runLineText(run({ end_reason: "restart" }), true)).toBe("Stopped when the computer restarted");
  });
  test("tell how long something lasted", () => {
    expect(lasted(42_000)).toBe("42 s");
    expect(lasted(12 * 60_000)).toBe("12 min");
    expect(lasted(125 * 60_000)).toBe("2 h 5 min");
  });
});
