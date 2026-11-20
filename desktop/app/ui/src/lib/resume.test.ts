import { describe, expect, test } from "vitest";
import { ApiError } from "../api/client";
import type { Run } from "../api/runtime";
import type { Presence } from "../api/types";
import { canOpenWhileRunning, canResume, resumeProblem } from "./resume";

const ended = { state: "ended" } as Run;
const running = { state: "running" } as Run;
const presence = (state: Presence["state"]) => ({ state }) as Presence;
const claude = { resume: true, while_running: true };
const codex = { resume: true, while_running: false };
const gemini = { resume: false, while_running: false };

describe("whether a session can be resumed", () => {
  test("a stopped session of an agent that resumes", () => {
    expect(canResume({ agent_session: "a1", resume: claude, presence: presence("stopped") }, [ended])).toBe(true);
    expect(canResume({ agent_session: "a1", resume: codex, presence: presence("stopped") }, [])).toBe(true);
    // no hooks, but its last run ended
    expect(canResume({ agent_session: "a1", resume: claude, presence: null }, [ended])).toBe(true);
  });
  test("not while it runs, before it stopped, or for another agent", () => {
    expect(canResume({ agent_session: "a1", resume: claude, presence: presence("stopped") }, [running])).toBe(false);
    expect(canResume({ agent_session: "a1", resume: claude, presence: presence("idle") }, [ended])).toBe(false);
    expect(canResume({ agent_session: "a1", resume: claude, presence: null }, [])).toBe(false);
    expect(canResume({ agent_session: "a1", resume: gemini, presence: presence("stopped") }, [ended])).toBe(false);
    expect(canResume(null, [])).toBe(false);
  });
});

describe("whether a session running elsewhere can be opened in Pinrail", () => {
  test("one that has not stopped, of an agent that allows a second process", () => {
    expect(canOpenWhileRunning({ agent_session: "a1", resume: claude, presence: presence("working") }, [])).toBe(true);
    expect(canOpenWhileRunning({ agent_session: "a1", resume: claude, presence: presence("idle") }, [ended])).toBe(
      true,
    );
    // no hooks and no run of Pinrail's tell whether it still runs
    expect(canOpenWhileRunning({ agent_session: "a1", resume: claude, presence: null }, [])).toBe(true);
  });
  test("not once stopped, while Pinrail runs it, after a run of Pinrail's ended, or for Codex", () => {
    expect(canOpenWhileRunning({ agent_session: "a1", resume: claude, presence: presence("stopped") }, [])).toBe(false);
    expect(canOpenWhileRunning({ agent_session: "a1", resume: claude, presence: presence("working") }, [running])).toBe(
      false,
    );
    expect(canOpenWhileRunning({ agent_session: "a1", resume: claude, presence: null }, [ended])).toBe(false);
    expect(canOpenWhileRunning({ agent_session: "a1", resume: codex, presence: presence("working") }, [])).toBe(false);
  });
  test("not before the agent reported its own id for the session", () => {
    expect(canOpenWhileRunning({ agent_session: null, resume: claude, presence: presence("working") }, [])).toBe(false);
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
