// The button in a work item's panel that starts an agent on the item at
// once, in a folder of the item's repository, with the item as its task.
import { afterEach, expect, test, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import type { StartableAgent } from "../api/runtime";
import { core } from "../testing/core";
import { itemDetail } from "../testing/items";
import { cleanup, render } from "../testing/render";
import { useToasts } from "../state/toasts";
import { ItemQuickStart } from "./ItemQuickStart";

const app = vi.hoisted(() => ({
  started: [] as Record<string, unknown>[],
  refused: null as string | null,
}));
vi.mock("../api/client", async (real) => (await import("../testing/core")).fakeClient(await real()));
vi.mock("../api/runtime", async (real) => {
  const actual = await real<typeof import("../api/runtime")>();
  const agent = (id: string, name: string, last_used: boolean): StartableAgent => ({
    id,
    name,
    program: id,
    installed: true,
    version: "1.0.0",
    last_used,
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
        return {
          run: {},
          session: { agent: String(body.agent), id: "ses_01NEW", agent_session: null },
          ...(app.refused ? { claim_refused: app.refused } : {}),
        };
      },
    },
  };
});

afterEach(() => {
  cleanup();
  core.reset();
  app.started = [];
  app.refused = null;
});

/** The toasts, which the app's layout draws. */
function Toasts() {
  return (
    <output data-testid="toasts">
      {useToasts()
        .toasts.map((t) => t.text)
        .join(" | ")}
    </output>
  );
}

const item = itemDetail({ title: "Fix the login", number: 7, body: "The redirect loops after a sign-in." });

test("starts the agent used last on the item, in the latest folder of its repository", async () => {
  await render(<ItemQuickStart item={item} />);
  const start = page.getByRole("button", { name: "Start Claude Code" });
  await expect.element(start).toBeVisible();
  await userEvent.click(start);
  await expect.poll(() => app.started.length).toBe(1);
  expect(app.started[0]).toMatchObject({
    agent: "claude-code",
    folder: "/work/app",
    item: "ACME-7",
    task: "# ACME-7 Fix the login\n\nThe redirect loops after a sign-in.",
  });
});

test("the menu changes the agent and the folder without starting", async () => {
  await render(<ItemQuickStart item={item} />);
  await userEvent.click(page.getByRole("button", { name: "Choose the agent and the folder" }));
  await userEvent.click(page.getByRole("menuitem", { name: /Codex/ }));
  await expect.element(page.getByRole("button", { name: "Start Codex" })).toBeVisible();
  await userEvent.click(page.getByRole("button", { name: "Choose the agent and the folder" }));
  // only the folders of the item's repository are offered
  await expect.element(page.getByRole("menuitem", { name: /api/ })).not.toBeInTheDocument();
  await userEvent.click(page.getByRole("menuitem", { name: /app-login/ }));
  expect(app.started).toEqual([]);
  await userEvent.click(page.getByRole("button", { name: "Start Codex" }));
  await expect.poll(() => app.started[0]).toMatchObject({ agent: "codex", folder: "/work/app-login" });
});

test("says why the new session could not claim the item", async () => {
  app.refused = "ACME-7 is on hold";
  await render(
    <>
      <ItemQuickStart item={item} />
      <Toasts />
    </>,
  );
  await userEvent.click(page.getByRole("button", { name: "Start Claude Code" }));
  await expect
    .element(page.getByTestId("toasts"))
    .toHaveTextContent("Claude Code started, but it could not claim ACME-7: ACME-7 is on hold");
});
