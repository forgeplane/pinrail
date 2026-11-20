import { describe, expect, test } from "vitest";
import type { Run } from "../../api/runtime";
import type { SessionTurn } from "../../api/types";
import { lasted, runLineText, turnEndText } from "./runLines";

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

const turn = (over: Partial<SessionTurn>): SessionTurn => ({
  turn: 3,
  agent_turn: null,
  started_at: "2026-11-20T07:22:00Z",
  ended_at: "2026-11-20T07:32:00Z",
  end_kind: "done",
  prompt: null,
  prompt_cut: false,
  reply: null,
  reply_cut: false,
  ...over,
});

describe("a turn that ended without a reply", () => {
  test("says how it ended, and after how long", () => {
    const name = "Antigravity CLI";
    expect(turnEndText(turn({ end_kind: "interrupted" }), name)).toBe("You interrupted Antigravity CLI after 10 min");
    expect(turnEndText(turn({ end_kind: "failed" }), name)).toBe("Antigravity CLI's turn failed after 10 min");
    expect(turnEndText(turn({ end_kind: "ended" }), name)).toBe(
      "The session ended during Antigravity CLI's turn, after 10 min",
    );
    expect(turnEndText(turn({ end_kind: "done" }), name)).toBe("Antigravity CLI finished the turn in 10 min");
    expect(turnEndText(turn({ end_kind: null }), name)).toBe("Antigravity CLI finished the turn in 10 min");
  });
  test("says nothing while the turn goes on", () => {
    expect(turnEndText(turn({ ended_at: null, end_kind: null }), "Codex")).toBeNull();
  });
});
