// The Start an agent dialog for an agent that tells Pinrail its session
// only through its hooks: it asks for Report status before it starts one,
// and opens the session as soon as the start answers.
import { useLocation } from "react-router";
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { StartableAgent } from "../api/runtime";
import { core } from "../testing/core";
import { cleanup, render } from "../testing/render";
import { NewSession } from "./NewSession";

const app = vi.hoisted(() => ({
  agents: [] as StartableAgent[],
  invoked: [] as [string, unknown][],
  started: [] as unknown[],
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
      // the session is Pinrail's at once; the agent names its own later
      start: async (body: unknown) => {
        app.started.push(body);
        return { run: {}, session: { agent: "antigravity", id: "ses_01NEW", agent_session: null } };
      },
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
  app.started = [];
});

function Where() {
  return <output data-testid="where">{useLocation().pathname}</output>;
}

const agent = (
  id: string,
  name: string,
  names_session: boolean,
  reports: boolean | null,
  installs: StartableAgent["installs"] = "hooks",
): StartableAgent => ({
  id,
  name,
  program: id,
  installed: true,
  version: "1.0.0",
  last_used: true,
  names_session,
  reports,
  installs,
});

const startButton = () => page.getByRole("button", { name: "Start", exact: true });

test("an agent that names its session only in its hooks starts once Report status is on", async () => {
  app.agents = [agent("antigravity", "Antigravity CLI", false, false)];
  await render(<NewSession />);
  await expect
    .element(page.getByText("Antigravity CLI needs Pinrail's hooks to start from Pinrail", { exact: false }))
    .toBeVisible();
  await expect.element(startButton()).toBeDisabled();

  await userEvent.click(page.getByRole("button", { name: "Install hooks" }));
  expect(app.invoked.filter(([command]) => command === "set_agent_hooks")).toEqual([
    ["set_agent_hooks", { id: "antigravity", on: true }],
  ]);
  await expect.element(page.getByText(/needs Pinrail's hooks/)).not.toBeInTheDocument();
  await expect.element(startButton()).toBeEnabled();
});

test("an agent whose session Pinrail names starts with its hooks off, and offers to turn them on", async () => {
  app.agents = [agent("grok", "Grok CLI", true, false)];
  await render(<NewSession />);
  await expect.element(startButton()).toBeEnabled();
  await expect.element(page.getByText(/needs Pinrail's/)).not.toBeInTheDocument();
  // what the hooks add, and the switch, without holding the start back
  const offer = page.getByTestId("new-session-reports-offer");
  await expect
    .element(offer.getByText("Pinrail's hooks show Grok CLI's turns, prompts and replies in its session."))
    .toBeVisible();
  await userEvent.click(offer.getByRole("button", { name: "Install hooks" }));
  expect(app.invoked.filter(([command]) => command === "set_agent_hooks")).toEqual([
    ["set_agent_hooks", { id: "grok", on: true }],
  ]);
  await expect.element(offer).not.toBeInTheDocument();
  await expect.element(startButton()).toBeEnabled();
});

test("an agent Pinrail reports through an extension is offered the extension", async () => {
  app.agents = [agent("pi", "Pi", true, false, "extension")];
  await render(<NewSession />);
  const offer = page.getByTestId("new-session-reports-offer");
  await expect
    .element(offer.getByText("Pinrail's extension shows Pi's turns, prompts and replies in its session."))
    .toBeVisible();
  await expect.element(offer.getByRole("button", { name: "Install extension" })).toBeVisible();
});

test("an agent Pinrail reports through a plugin is offered the plugin", async () => {
  app.agents = [agent("opencode", "OpenCode", true, false, "plugin")];
  await render(<NewSession />);
  const offer = page.getByTestId("new-session-reports-offer");
  await expect
    .element(offer.getByText("Pinrail's plugin shows OpenCode's turns, prompts and replies in its session."))
    .toBeVisible();
  await expect.element(offer.getByRole("button", { name: "Install plugin" })).toBeVisible();
});

test("an agent whose hooks are on is offered nothing", async () => {
  app.agents = [agent("grok", "Grok CLI", true, true)];
  await render(<NewSession />);
  await expect.element(startButton()).toBeEnabled();
  await expect.element(page.getByTestId("new-session-reports-offer")).not.toBeInTheDocument();
});

test("a start opens its session at once, before the agent names it", async () => {
  app.agents = [agent("antigravity", "Antigravity CLI", false, true)];
  await render(
    <>
      <NewSession />
      <Where />
    </>,
    { route: "/new" },
  );
  await expect.element(startButton()).toBeEnabled();
  await userEvent.click(startButton());
  await expect.element(page.getByTestId("where")).toHaveTextContent("/sessions/antigravity/ses_01NEW");
  expect(app.started).toEqual([expect.objectContaining({ agent: "antigravity", folder: "/work/api" })]);
});
