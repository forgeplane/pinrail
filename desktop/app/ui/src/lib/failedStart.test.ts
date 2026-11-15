import { describe, expect, test } from "vitest";
import type { Run } from "../api/runtime";
import { endedText, exitText, failedAtStart } from "./failedStart";

const run = (over: Partial<Run>): Run => ({
  id: "run_1",
  host: "local",
  agent: "claude-code",
  state: "ended",
  terminal: "local:t_1",
  folder: "/work",
  started_at: "2026-11-14T10:00:00.000Z",
  ended_at: "2026-11-14T10:00:01.200Z",
  end_reason: "exited",
  exit_code: 1,
  exit_signal: null,
  session: null,
  ...over,
});

describe("a run that failed as it started", () => {
  test("exited with an error within 2 seconds", () => {
    expect(failedAtStart(run({}))).toBe(true);
    expect(failedAtStart(run({ exit_code: null, exit_signal: "KILL" }))).toBe(true);
  });
  test("is not one that exited cleanly, later, was stopped or still runs", () => {
    expect(failedAtStart(run({ exit_code: 0 }))).toBe(false);
    expect(failedAtStart(run({ ended_at: "2026-11-14T10:00:02.500Z" }))).toBe(false);
    expect(failedAtStart(run({ end_reason: "stopped" }))).toBe(false);
    expect(failedAtStart(run({ state: "running", ended_at: null }))).toBe(false);
  });
  test("is told with its code or signal", () => {
    expect(exitText("Claude Code", run({}))).toBe("Claude Code exited with code 1 as it started.");
    expect(exitText("Codex", run({ exit_code: null, exit_signal: "KILL" }))).toBe(
      "Codex was ended by signal KILL as it started.",
    );
  });
});

describe("the banner of an ended run", () => {
  test("says how it ended", () => {
    expect(endedText("Claude Code", run({ exit_code: 0 }))).toBe("Claude Code exited with code 0.");
    expect(endedText("Claude Code", run({ end_reason: "stopped", exit_code: null, exit_signal: "TERM" }))).toBe(
      "Claude Code was stopped by you.",
    );
    expect(endedText("Codex", run({ end_reason: "host_lost", exit_code: null }))).toBe(
      "Codex stopped when Pinrail lost its terminal host, such as when the computer restarted.",
    );
    expect(endedText("Codex", run({ exit_code: null, exit_signal: "KILL" }))).toBe("Codex was ended by signal KILL.");
  });
});

describe("a resume that failed", () => {
  test("is one that failed as it started, and says it was resuming", () => {
    const resumed = run({ kind: "resume", end_reason: "resume_failed", ended_at: "2026-11-14T10:00:02.800Z" });
    expect(failedAtStart(resumed)).toBe(true);
    expect(exitText("Claude Code", resumed)).toBe("Claude Code exited right after resuming (code 1).");
  });
});
