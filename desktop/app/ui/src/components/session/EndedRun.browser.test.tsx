// A run that ended, in a session's Terminal view: the app's empty state,
// with how and when it ended, and Resume.
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { Run } from "../../api/runtime";
import { cleanup, render } from "../../testing/render";
import { EndedRun, lasted } from "./EndedRun";

vi.mock("../../api/runtime", async (real) => {
  const actual = await real<typeof import("../../api/runtime")>();
  return { ...actual, runtime: { ...actual.runtime, screen: async () => Promise.reject(new Error("no terminal")) } };
});

afterEach(() => cleanup());

const ago = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();

const run = {
  id: "run_1",
  host: "local",
  agent: "claude-code",
  state: "ended",
  terminal: null,
  folder: "/work/app",
  started_at: ago(83),
  ended_at: ago(60),
  end_reason: "stopped",
  exit_code: null,
  exit_signal: "TERM",
  session: { agent: "claude-code", id: "ses_1", agent_session: "a1" },
} as Run;

test("says how and when the run ended, that its screen is gone, and resumes it", async () => {
  const onResume = vi.fn();
  await render(<EndedRun run={run} onResume={onResume} />);
  await expect.element(page.getByRole("heading", { name: "Claude Code was stopped by you" })).toBeVisible();
  await expect
    .element(
      page.getByText(
        "It ended 1h ago, after 23 minutes. Pinrail no longer keeps its last screen. Resume it to go on where it left off.",
      ),
    )
    .toBeVisible();
  await userEvent.click(page.getByRole("button", { name: "Resume" }));
  expect(onResume).toHaveBeenCalledOnce();
});

test("offers no Resume when the session cannot be resumed", async () => {
  await render(<EndedRun run={run} />);
  await expect.element(page.getByText("Pinrail no longer keeps its last screen.", { exact: false })).toBeVisible();
  await expect.element(page.getByRole("button", { name: "Resume" })).not.toBeInTheDocument();
});

test("says how long a run lasted in words", () => {
  const at = (s: number) => new Date(Date.UTC(2026, 0, 1, 0, 0, s)).toISOString();
  expect(lasted(at(0), at(40))).toBe("40 seconds");
  expect(lasted(at(0), at(60))).toBe("1 minute");
  expect(lasted(at(0), at(23 * 60))).toBe("23 minutes");
  expect(lasted(at(0), at(65 * 60))).toBe("1 hour 5 minutes");
  expect(lasted(at(0), at(120 * 60))).toBe("2 hours");
});
