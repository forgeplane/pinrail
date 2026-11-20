// The banner on the Sessions page after the computer restarted: the agents
// the restart stopped, resumed all at once or reviewed. The shell's core
// has no terminal host, so the stopped runs and the resumes are stood in
// for.

import { expect, test, type Page } from "../fixtures";
import { core } from "../helpers";

test.beforeEach(async ({ page }) => {
  await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: true } } });
});
test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
  await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: false } } });
});

function stopped(id: string, session: string, title: string | null, agentSession: string | null = `agent-${session}`) {
  return {
    id,
    host: "local",
    agent: "claude-code",
    kind: "start",
    state: "ended",
    terminal: `local:t_${id}`,
    folder: "/work/api",
    started_at: "2026-11-15T08:00:00Z",
    ended_at: "2026-11-15T09:00:00Z",
    end_reason: "restart",
    exit_code: null,
    exit_signal: null,
    session: { agent: "claude-code", id: session, agent_session: agentSession },
    title,
  };
}

/** Stands in for the stopped runs and for resuming and dismissing them;
 *  `refuse` names sessions whose resume is refused. */
async function standIn(page: Page, refuse: string[] = []) {
  let runs = [
    stopped("run_a", "s-a", "Fix the login redirect"),
    stopped("run_b", "s-b", null),
    // its agent never reported its own id, so it cannot be resumed
    stopped("run_c", "s-c", "Never named", null),
  ];
  const resumed: string[] = [];
  let dismissed = 0;
  await page.route(/\/api\/v1\/runtime\/restarted$/, (route) => {
    if (route.request().method() === "DELETE") {
      dismissed++;
      runs = [];
      return route.fulfill({ status: 204, body: "" });
    }
    return route.fulfill({ json: { runs } });
  });
  await page.route(/\/api\/v1\/sessions\/claude-code\/[^/]+\/resume$/, async (route) => {
    const session = route.request().url().split("/").at(-2)!;
    resumed.push(session);
    if (refuse.includes(session)) {
      return route.fulfill({
        status: 409,
        json: {
          error: "already_running",
          message: "This session is already running in another terminal.",
          violations: [],
          run: null,
        },
      });
    }
    runs = runs.filter((r) => r.session.id !== session);
    return route.fulfill({
      status: 201,
      json: {
        run: { ...stopped("run_new", session, null), state: "running" },
        session: { agent: "claude-code", id: session, agent_session: `agent-${session}` },
      },
    });
  });
  return { resumed, dismissed: () => dismissed };
}

const banner = (page: Page) => page.getByTestId("restart-banner");

test("Resume all resumes each agent a restart stopped, one by one", async ({ page }) => {
  const { resumed } = await standIn(page);
  await page.goto("/#/sessions");
  await expect(banner(page)).toContainText("2 agents stopped when the computer restarted.");

  await banner(page).getByRole("button", { name: "Review" }).click();
  const list = banner(page).getByRole("list");
  await expect(list.getByRole("link")).toHaveText(["Fix the login redirect", "Claude Code session agent-s-b"]);

  await banner(page).getByRole("button", { name: "Resume all" }).click();
  await expect(banner(page)).toHaveCount(0);
  expect(resumed).toEqual(["s-a", "s-b"]);
});

test("a resume that cannot happen is named, and the banner can be dismissed", async ({ page }) => {
  const { resumed, dismissed } = await standIn(page, ["s-a"]);
  await page.goto("/#/sessions");
  await banner(page).getByRole("button", { name: "Resume all" }).click();
  await expect(banner(page).getByRole("alert")).toHaveText(
    "Fix the login redirect: This session is already running in another terminal.",
  );
  expect(resumed).toEqual(["s-a", "s-b"]);
  await expect(banner(page)).toContainText("1 agent stopped when the computer restarted.");

  await banner(page).getByRole("button", { name: "Dismiss" }).click();
  await expect(banner(page)).toHaveCount(0);
  expect(dismissed()).toBe(1);
});
