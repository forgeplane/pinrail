import { describe, expect, test } from "vitest";
import { ApiError } from "../api/client";
import type { Run } from "../api/runtime";
import type { Presence } from "../api/types";
import { canResume, resumeProblem } from "./resume";

const ended = { state: "ended" } as Run;
const running = { state: "running" } as Run;
const presence = (state: Presence["state"]) => ({ state }) as Presence;

describe("whether a session can be resumed", () => {
  test("a stopped session of an agent that resumes", () => {
    expect(canResume({ agent: "claude-code", presence: presence("stopped") }, [ended])).toBe(true);
    expect(canResume({ agent: "codex", presence: presence("stopped") }, [])).toBe(true);
    // no hooks, but its last run ended
    expect(canResume({ agent: "claude-code", presence: null }, [ended])).toBe(true);
  });
  test("not while it runs, before it stopped, or for another agent", () => {
    expect(canResume({ agent: "claude-code", presence: presence("stopped") }, [running])).toBe(false);
    expect(canResume({ agent: "claude-code", presence: presence("idle") }, [ended])).toBe(false);
    expect(canResume({ agent: "claude-code", presence: null }, [])).toBe(false);
    expect(canResume({ agent: "gemini", presence: presence("stopped") }, [ended])).toBe(false);
    expect(canResume(null, [])).toBe(false);
  });
});

describe("why a resume did not happen", () => {
  test("a missing folder is named", () => {
    const error = new ApiError(422, { error: "folder_missing", message: "/work does not exist" });
    expect(resumeProblem(error, "/work")).toBe(
      "The folder /work no longer exists, so the session cannot be resumed here.",
    );
  });
  test("otherwise the core's words", () => {
    const error = new ApiError(409, {
      error: "already_running",
      message: "This session is already running in another terminal.",
    });
    expect(resumeProblem(error, "/work")).toBe("This session is already running in another terminal.");
  });
});
