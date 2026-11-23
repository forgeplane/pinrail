// The quick add's box: adding an item, and adding it and starting an agent
// on it at once.
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { StartableAgent } from "../api/runtime";
import type { ServerSettings, WorkItem } from "../api/types";
import { core } from "../testing/core";
import { itemDetail, workItem } from "../testing/items";
import { cleanup, render } from "../testing/render";
import { QuickAddBox } from "./QuickAddBox";

const app = vi.hoisted(() => ({ started: [] as Record<string, unknown>[] }));
vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));
vi.mock("../api/runtime", async (real) => {
  const actual = await real<typeof import("../api/runtime")>();
  const agent = (id: string, name: string, preferred: boolean): StartableAgent => ({
    id,
    name,
    program: id,
    installed: true,
    version: "1.0.0",
    preferred,
  });
  return {
    ...actual,
    runtime: {
      ...actual.runtime,
      startable: async () => [agent("claude-code", "Claude Code", true), agent("codex", "Codex", false)],
      recentFolders: async () => [
        { folder: "/work/api", repo: "acme/api", branch: "main", exists: true, used_at: "" },
        { folder: "/work/app", repo: "acme/app", branch: "main", exists: true, used_at: "" },
        { folder: "/work/app-login", repo: "acme/app", branch: "fix/login", exists: true, used_at: "" },
      ],
      start: async (body: Record<string, unknown>) => {
        app.started.push(body);
        return { run: {}, session: { agent: String(body.agent), id: "ses_01NEW", agent_session: null } };
      },
    },
  };
});

afterEach(() => {
  cleanup();
  core.reset();
  app.started = [];
});

/** The settings as the core serves them, with the agent runtime on or off. */
const served = (runtime: boolean) =>
  ({
    appearance: { theme: "system", dark_theme: "dark", light_theme: "light", text_size: "default" },
    sidebar: { open: true },
    notifications: { enabled: true, paused_until: null, sound: true, muted_plugins: [] },
    shortcut: { global: "", global_opens: "oldest" },
    port: 4747,
    labs: { runtime },
  }) as unknown as ServerSettings;

const added = workItem({ title: "Fix the login", number: 7 });

async function open(runtime = true) {
  core.answer("settings", served(runtime));
  core.answer("addItem", added);
  core.answer("item", itemDetail({ title: "Fix the login", number: 7, body: "The redirect loops." }));
  const onAdded = vi.fn<(item: WorkItem, more: boolean, started: boolean) => void>();
  await render(
    <QuickAddBox
      onClose={() => {}}
      repos={["acme/api", "acme/app"]}
      items={[]}
      defaultRepo="acme/app"
      defaultParent={null}
      onAdded={onAdded}
    />,
  );
  return onAdded;
}

test("⌥Enter adds the item and starts the preferred agent on it", async () => {
  const onAdded = await open();
  await expect.element(page.getByRole("button", { name: "Add and start Claude Code" })).toBeVisible();
  await userEvent.keyboard("Fix the login");
  await userEvent.keyboard("{Alt>}{Enter}{/Alt}");
  await expect.poll(() => app.started.length).toBe(1);
  expect(core.calls("addItem")[0][0]).toMatchObject({ repo: "acme/app", title: "Fix the login" });
  expect(app.started[0]).toMatchObject({
    agent: "claude-code",
    folder: "/work/app",
    item: "ACME-7",
    task: "# ACME-7 Fix the login\n\nThe redirect loops.",
  });
  await expect.poll(() => onAdded.mock.calls[0]).toEqual([added, false, true]);
});

test("the button's menu changes the agent and the folder of the start", async () => {
  const onAdded = await open();
  await userEvent.keyboard("Fix the login");
  await userEvent.click(page.getByRole("button", { name: "Choose the agent and the folder" }));
  await userEvent.click(page.getByRole("menuitem", { name: /Codex/ }));
  await userEvent.click(page.getByRole("button", { name: "Choose the agent and the folder" }));
  await userEvent.click(page.getByRole("menuitem", { name: /app-login/ }));
  expect(core.calls("addItem")).toEqual([]);
  await userEvent.click(page.getByRole("button", { name: "Add and start Codex" }));
  await expect.poll(() => app.started[0]).toMatchObject({ agent: "codex", folder: "/work/app-login" });
  expect(onAdded).toHaveBeenCalledWith(added, false, true);
});

test("Enter only adds the item", async () => {
  const onAdded = await open();
  await expect.element(page.getByRole("button", { name: "Add and start Claude Code" })).toBeVisible();
  await userEvent.keyboard("Fix the login{Enter}");
  await expect.poll(() => onAdded.mock.calls[0]).toEqual([added, false, false]);
  expect(app.started).toEqual([]);
});

test("the Add more button adds the item and keeps the box open for another", async () => {
  const onAdded = await open();
  await userEvent.keyboard("Fix the login");
  await userEvent.click(page.getByRole("button", { name: "Add more" }));
  await expect.poll(() => onAdded.mock.calls[0]).toEqual([added, true, false]);
  expect(app.started).toEqual([]);
});

test("offers no start while the agent runtime is off", async () => {
  await open(false);
  await expect.element(page.getByRole("textbox", { name: "What needs to be done?" })).toBeVisible();
  await expect.element(page.getByRole("button", { name: /Add and start/ })).not.toBeInTheDocument();
});
