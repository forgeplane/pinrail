// The Start an agent dialog for an agent that tells Pinrail its session
// only through its hooks: it asks for Report status before it starts one.
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { StartableAgent } from "../api/runtime";
import { core } from "../testing/core";
import { cleanup, render } from "../testing/render";
import { NewSession } from "./NewSession";

const app = vi.hoisted(() => ({
  agents: [] as StartableAgent[],
  invoked: [] as [string, unknown][],
}));
vi.mock("../api/client", async (real) => ({
  ...(await import("../testing/core")).fakeClient(await real()),
  inTauri: () => true,
}));
vi.mock("../api/runtime", async (real) => {
  const actual = await real<typeof import("../api/runtime")>();
  return {
    ...actual,
    runtime: {
      ...actual.runtime,
      startable: async () => app.agents.map((a) => ({ ...a })),
      recentFolders: async () => [{ folder: "/work/api", repo: "acme/api", branch: "main", exists: true }],
    },
  };
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (command: string, args: unknown) => {
    app.invoked.push([command, args]);
    // the hooks are on from now on
    if (command === "set_agent_hooks") app.agents = app.agents.map((a) => ({ ...a, reports: true }));
    return [];
  },
}));

afterEach(() => {
  cleanup();
  core.reset();
  app.invoked = [];
});

const agent = (id: string, name: string, names_session: boolean, reports: boolean | null): StartableAgent => ({
  id,
  name,
  program: id,
  installed: true,
  version: "1.0.0",
  last_used: true,
  names_session,
  reports,
});

const startButton = () => page.getByRole("button", { name: "Start", exact: true });

test("an agent that names its session only in its hooks starts once Report status is on", async () => {
  app.agents = [agent("antigravity", "Antigravity CLI", false, false)];
  await render(<NewSession />);
  await expect.element(page.getByText(/needs Report status/)).toBeVisible();
  await expect.element(startButton()).toBeDisabled();

  await userEvent.click(page.getByRole("button", { name: "Turn on Report status" }));
  expect(app.invoked.filter(([command]) => command === "set_agent_hooks")).toEqual([
    ["set_agent_hooks", { id: "antigravity", on: true }],
  ]);
  await expect.element(page.getByText(/needs Report status/)).not.toBeInTheDocument();
  await expect.element(startButton()).toBeEnabled();
});

test("an agent whose session Pinrail names starts with its hooks off", async () => {
  app.agents = [agent("claude-code", "Claude Code", true, false)];
  await render(<NewSession />);
  await expect.element(startButton()).toBeEnabled();
  await expect.element(page.getByText(/needs Report status/)).not.toBeInTheDocument();
});
