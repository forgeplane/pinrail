// The Start an agent dialog for an agent that tells Pinrail its session
// only through its hooks: it asks to connect the agent before it starts one,
// and opens the session as soon as the start answers.
import { useLocation } from "react-router";
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { StartableAgent } from "../api/runtime";
import { core } from "../testing/core";
import { cleanup, render } from "../testing/render";
import { workItem } from "../testing/items";
import { NewSession } from "./NewSession";
import { QuickAddDialog } from "./QuickAddDialog";

const app = vi.hoisted(() => ({
  agents: [] as StartableAgent[],
  // what a check made afresh finds, when it differs from the kept list
  fresh: null as StartableAgent[] | null,
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
      startable: async (fresh = false) => ((fresh && app.fresh) || app.agents).map((a) => ({ ...a })),
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
    // connected, its hooks are on from now on
    if (command === "connect_agent") app.agents = app.agents.map((a) => ({ ...a, reports: true }));
    return [];
  },
}));

afterEach(() => {
  cleanup();
  core.reset();
  app.invoked = [];
  app.started = [];
  app.fresh = null;
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
  preferred: true,
  names_session,
  reports,
  installs,
});

const startButton = () => page.getByRole("button", { name: "Start", exact: true });

test("an agent that names its session only in its hooks starts once it is connected", async () => {
  app.agents = [agent("antigravity", "Antigravity CLI", false, false)];
  await render(<NewSession />);
  await expect
    .element(page.getByText("Connect Antigravity CLI to start it from Pinrail", { exact: false }))
    .toBeVisible();
  await expect.element(startButton()).toBeDisabled();

  await userEvent.click(page.getByRole("button", { name: "Connect Antigravity CLI" }));
  expect(app.invoked.filter(([command]) => command === "connect_agent")).toEqual([
    ["connect_agent", { id: "antigravity" }],
  ]);
  await expect.element(page.getByText(/to start it from Pinrail/)).not.toBeInTheDocument();
  await expect.element(startButton()).toBeEnabled();
});

test("an agent whose session Pinrail names starts unconnected, and offers to connect it", async () => {
  app.agents = [agent("grok", "Grok CLI", true, false)];
  await render(<NewSession />);
  await expect.element(startButton()).toBeEnabled();
  await expect.element(page.getByText(/to start it from Pinrail/)).not.toBeInTheDocument();
  // what connecting adds, without holding the start back
  const offer = page.getByTestId("new-session-reports-offer");
  await expect
    .element(offer.getByText("Connect Grok CLI to see its turns, prompts and replies in its session."))
    .toBeVisible();
  await userEvent.click(offer.getByRole("button", { name: "Connect Grok CLI" }));
  expect(app.invoked.filter(([command]) => command === "connect_agent")).toEqual([["connect_agent", { id: "grok" }]]);
  await expect.element(offer).not.toBeInTheDocument();
  await expect.element(startButton()).toBeEnabled();
});

test("an agent installed since the kept check can be chosen once the fresh one answers", async () => {
  app.agents = [{ ...agent("grok", "Grok CLI", true, true), installed: false, version: null }];
  app.fresh = [agent("grok", "Grok CLI", true, true)];
  await render(<NewSession />);
  await expect.element(startButton()).toBeEnabled();
});

test("a connected agent is offered nothing", async () => {
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

test("Enter in the first message starts the session with that message", async () => {
  app.agents = [agent("antigravity", "Antigravity CLI", false, true)];
  await render(<NewSession />, { route: "/new" });
  await expect.element(startButton()).toBeEnabled();
  await userEvent.click(page.getByRole("textbox", { name: "First message" }));
  await userEvent.keyboard("Fix the login{Enter}");
  await expect.poll(() => app.started).toEqual([expect.objectContaining({ task: "Fix the login" })]);
});

test("Shift+Enter in the first message starts a new line", async () => {
  app.agents = [agent("antigravity", "Antigravity CLI", false, true)];
  await render(<NewSession />, { route: "/new" });
  await expect.element(startButton()).toBeEnabled();
  const message = page.getByRole("textbox", { name: "First message" });
  await userEvent.click(message);
  await userEvent.keyboard("Fix the login{Shift>}{Enter}{/Shift}on Safari");
  await expect.element(message).toHaveValue("Fix the login\non Safari");
  expect(app.started).toEqual([]);
});

test("⌘Enter in the quick add dialog adds the item and starts no session", async () => {
  app.agents = [agent("antigravity", "Antigravity CLI", false, true)];
  core.answer("projects", { projects: [], loose: [{ repo: "acme/app" }] });
  core.answer("items", { items: [] });
  core.answer("addItem", (body: unknown) => workItem({ title: (body as { title: string }).title, number: 1 }));
  await render(
    <>
      <NewSession />
      <QuickAddDialog onClose={() => {}} />
      <Where />
    </>,
    { route: "/new" },
  );
  await expect.element(startButton()).toBeEnabled();
  const title = page.getByRole("dialog", { name: "Add a work item" }).getByRole("textbox").first();
  await userEvent.click(title);
  await userEvent.keyboard("Fix the login");
  await userEvent.keyboard("{ControlOrMeta>}{Enter}{/ControlOrMeta}");
  await expect.poll(() => core.calls("addItem")).toHaveLength(1);
  expect(app.started).toEqual([]);
  await expect.element(page.getByTestId("where")).toHaveTextContent("/new");
});

test("the agent picker says which arguments the agent starts with", async () => {
  core.answer("settings", {
    appearance: { theme: "system" },
    sidebar: { open: true },
    notifications: { enabled: true, paused_until: null, sound: true, muted_plugins: [] },
    shortcut: { global: "", global_opens: "oldest" },
    plugins: {},
    port: 4747,
    agents: { auto_update: true, args: { grok: "--model grok-5 --yolo" } },
  });
  app.agents = [agent("grok", "Grok CLI", true, true)];
  await render(<NewSession />);
  await userEvent.hover(page.getByRole("button", { name: "Agent: Grok CLI" }));
  await expect
    .element(page.getByRole("tooltip"))
    .toHaveTextContent("Starts with --model grok-5 --yolo, from Settings › Agents");
});
