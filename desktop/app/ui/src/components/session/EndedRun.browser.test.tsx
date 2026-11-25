// A run that ended, in a session's Terminal view: the app's empty state,
// with how and when it ended, and Resume.
import { useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { Run } from "../../api/runtime";
import { cleanup, render } from "../../testing/render";
import { EndedRun, lasted } from "./EndedRun";

// the screens the host keeps, by run; none unless a test adds one
const app = vi.hoisted(() => ({
  screens: new Map<string, Promise<{ paint: string; cols: number; rows: number }>>(),
  themes: [] as string[],
}));
vi.mock("../../api/runtime", async (real) => {
  const actual = await real<typeof import("../../api/runtime")>();
  return {
    ...actual,
    runtime: {
      ...actual.runtime,
      screen: async (id: string) => app.screens.get(id) ?? Promise.reject(new Error("no terminal")),
    },
  };
});
// a renderer that notes each theme it is given
vi.mock("../terminal/renderers", async () => {
  const { referenceRenderer } = await import("../terminal/testing/reference");
  const renderer = {
    ...referenceRenderer,
    id: "ghostty",
    create: () => {
      const view = referenceRenderer.create();
      const setTheme = view.setTheme.bind(view);
      view.setTheme = (theme) => {
        app.themes.push(theme.background);
        setTheme(theme);
      };
      return view;
    },
  };
  return { RENDERERS: { ghostty: async () => renderer } };
});

afterEach(() => {
  cleanup();
  app.screens.clear();
  app.themes = [];
});

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

test("the last screen takes the app's theme when it changes", async () => {
  app.screens.set("run_1", Promise.resolve({ paint: "the last words", cols: 20, rows: 5 }));
  await render(<EndedRun run={run} />);
  await expect.element(page.getByRole("img", { name: "Claude Code's last screen" })).toBeVisible();
  window.dispatchEvent(new Event("pinrail:appearance"));
  await expect.poll(() => app.themes.length).toBe(1);
});

test("another run's screen is not shown while this run's loads", async () => {
  app.screens.set("run_1", Promise.resolve({ paint: "the first run", cols: 20, rows: 5 }));
  // the second run's screen never comes
  app.screens.set("run_2", new Promise(() => {}));
  function Switch() {
    const [id, setId] = useState("run_1");
    return (
      <>
        <button type="button" onClick={() => setId("run_2")}>
          Next run
        </button>
        <EndedRun run={{ ...run, id }} />
      </>
    );
  }
  await render(<Switch />);
  await expect.element(page.getByRole("img", { name: "Claude Code's last screen" })).toBeVisible();
  await userEvent.click(page.getByRole("button", { name: "Next run" }));
  await expect.element(page.getByRole("img", { name: "Claude Code's last screen" })).not.toBeInTheDocument();
});
