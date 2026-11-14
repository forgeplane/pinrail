// The Start an agent dialog. The shell's core has no terminal host, so the
// agents, the folders, the started run and its terminal are stood in for;
// a flag that turns permission checks off goes to the real core, which
// refuses it before it needs a host.

import { expect, test, type Page } from "./fixtures";
import { core } from "./helpers";

let n = 0;
const fresh = () => `s-start-${Date.now()}-${++n}`;

test.beforeEach(async ({ page }) => {
  await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: true } } });
});
test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
  await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: false } } });
});

const AGENTS = [
  { id: "claude-code", name: "Claude Code", program: "claude", installed: true, version: "2.1.325", last_used: true },
  { id: "codex", name: "Codex", program: "codex", installed: true, version: "0.98.0", last_used: false },
];

/** Stands in for the agents, the folders, the run and its terminal; the
 *  session `bindAfter` polls of the run, for an agent that names it late. */
async function standIn(page: Page, session: string, bindAfter = 0) {
  await page.route(/\/api\/v1\/agents\/startable/, (route) => route.fulfill({ json: { agents: AGENTS } }));
  await page.route(/\/api\/v1\/folders\/recent/, (route) =>
    route.fulfill({
      json: {
        folders: [
          { folder: "/work/api", repo: "acme/api", branch: "main", exists: true, used_at: "2026-11-14T10:00:00Z" },
          { folder: "/work/old", repo: null, branch: null, exists: false, used_at: "2026-11-01T10:00:00Z" },
        ],
      },
    }),
  );
  const started: Record<string, unknown>[] = [];
  let polls = 0;
  const run = (named: boolean) => ({
    id: "run_1",
    host: "local",
    agent: "claude-code",
    state: "running",
    terminal: "local:t_1",
    folder: "/work/api",
    started_at: new Date().toISOString(),
    ended_at: null,
    end_reason: null,
    exit_code: null,
    session: named ? { agent: "claude-code", id: session } : null,
  });
  await page.route(/\/api\/v1\/runs(\?|$)/, async (route) => {
    if (route.request().method() === "POST") {
      started.push(route.request().postDataJSON());
      return route.fulfill({
        status: 201,
        json: { run: run(bindAfter === 0), session: bindAfter === 0 ? { agent: "claude-code", id: session } : null },
      });
    }
    return route.fulfill({ json: { runs: started.length ? [run(true)] : [] } });
  });
  await page.route(/\/api\/v1\/runs\/run_1$/, (route) => route.fulfill({ json: run(++polls >= bindAfter) }));
  await page.route(/\/api\/v1\/terminals\/[^/]+\/tickets/, (route) => route.fulfill({ json: { ticket: "t" } }));
  const attached: URL[] = [];
  await page.routeWebSocket(/\/api\/v1\/terminals\/[^/]+\/attach/, (ws) => {
    attached.push(new URL(ws.url()));
    ws.send(JSON.stringify({ t: "hello", v: 1, role: "observer", cols: 80, rows: 24 }));
    ws.send(JSON.stringify({ t: "snapshot_begin", format: "vt", cols: 80, rows: 24, cut: 0 }));
    ws.send(Buffer.concat([Buffer.from([0x02]), Buffer.from("agent ready\r\n")]));
    ws.send(JSON.stringify({ t: "snapshot_end" }));
  });
  return { started, attached, polls: () => polls };
}

async function aSession(page: Page) {
  const session = fresh();
  const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { kind: "start", text: "Fix the login redirect", origin: { repo: "acme/api" } },
  });
  expect(response.status(), await response.text()).toBeLessThan(300);
  return session;
}

const dialog = (page: Page) => page.getByRole("dialog", { name: "Start an agent" });

test("starting Claude Code opens its session in the terminal view at the size it fits", async ({ page }) => {
  const session = await aSession(page);
  const { started, attached } = await standIn(page, session);
  await page.goto("/#/");
  await page.getByRole("button", { name: "Start an agent" }).click();
  const box = dialog(page);
  await expect(box).toBeVisible();

  // the most recent folder that exists, and the agent used last
  await expect(box.getByRole("combobox", { name: "Folder" })).toHaveValue("/work/api");
  await expect(box.getByRole("radio", { name: /Claude Code/ })).toHaveAttribute("aria-checked", "true");
  await expect(box.getByRole("radio", { name: /Codex/ })).toContainText("0.98.0");
  await box.getByRole("combobox", { name: "Folder" }).click();
  await expect(box.getByRole("option", { name: /\/work\/old/ })).toContainText("This folder no longer exists.");

  await box.getByLabel("Task").fill("Fix the login redirect");
  await box.getByText("More").click();
  await box.getByLabel("Extra arguments").fill('--model opus --name "login fix"');
  await page.keyboard.press("ControlOrMeta+Enter");

  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}`));
  await expect(page.getByTestId("terminal-bar")).toBeVisible();
  expect(started).toHaveLength(1);
  expect(started[0]).toMatchObject({
    agent: "claude-code",
    folder: "/work/api",
    task: "Fix the login redirect",
    args: ["--model", "opus", "--name", "login fix"],
  });
  // the size the run starts at is the one its terminal fits
  await expect.poll(() => attached.length).toBeGreaterThan(0);
  const fit = attached[0].searchParams;
  expect(Math.abs(Number(started[0].cols) - Number(fit.get("cols")))).toBeLessThanOrEqual(2);
  expect(Math.abs(Number(started[0].rows) - Number(fit.get("rows")))).toBeLessThanOrEqual(2);
});

test("an agent that names its session once it starts is waited for", async ({ page }) => {
  const session = await aSession(page);
  const { polls } = await standIn(page, session, 3);
  await page.goto("/#/");
  // the window knows the runtime is on once its settings are loaded
  await expect(page.getByRole("button", { name: "Start an agent" })).toBeVisible();
  await page.keyboard.press("ControlOrMeta+n");
  const box = dialog(page);
  await expect(box).toBeVisible();
  // 2 picks the second agent
  await box.getByRole("radio", { name: /Codex/ }).waitFor();
  await box.getByRole("heading", { name: "Start an agent" }).click();
  await page.keyboard.press("2");
  await expect(box.getByRole("radio", { name: /Codex/ })).toHaveAttribute("aria-checked", "true");
  await box.getByRole("button", { name: "Start" }).click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}`));
  expect(polls()).toBeGreaterThanOrEqual(3);
});

test("a flag that turns permission checks off is refused", async ({ page }) => {
  await page.route(/\/api\/v1\/agents\/startable/, (route) => route.fulfill({ json: { agents: AGENTS } }));
  await page.goto("/#/");
  await page.getByRole("button", { name: "Start an agent" }).click();
  const box = dialog(page);
  await box.getByRole("combobox", { name: "Folder" }).fill("/tmp");
  await box.getByText("More").click();
  await box.getByLabel("Extra arguments").fill("--dangerously-skip-permissions");
  await box.getByRole("button", { name: "Start" }).click();
  await expect(box.getByRole("alert")).toHaveText(
    "Pinrail does not start agents with permission checks turned off (--dangerously-skip-permissions).",
  );
  await expect(box).toBeVisible();
});
