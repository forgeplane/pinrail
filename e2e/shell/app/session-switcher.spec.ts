// The command palette's Sessions list, which ⌘P opens: the sessions that
// need the person first, opened on the current session. The list of
// sessions is stood in for, since the shared core holds the sessions of
// every other test; the current session is a real one, for its page.

import { expect, test, type Page } from "../fixtures";
import { clearInbox, core, createReview, sessionId } from "../helpers";

const ago = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();

function session(id: string, title: string, last: number, extra: Record<string, unknown> = {}) {
  return {
    agent: "claude-code",
    id,
    agent_session: id,
    repo: "acme/api",
    ref: "main",
    folder: "/work/api",
    status: null,
    title,
    first_at: ago(600),
    last_at: ago(last),
    total: { questions: 0, reviews: 0 },
    waiting: { questions: 0, reviews: 0 },
    ...extra,
  };
}
const hooked = (attention: string, presence: Record<string, unknown> = {}) => ({
  attention,
  presence: { attention, state: "idle", since: ago(1), turn: 1, turn_started: null, wait: null, ...presence },
});

/** A real session to stand on, and the sessions the palette lists around it. */
async function standIn(page: Page) {
  const agentSession = `s-switch-${Date.now()}`;
  const started = await page.request.post(`${core}/api/v1/sessions/claude-code/${agentSession}/status`, {
    data: { text: "Session switcher", kind: "start", origin: { repo: "acme/api" } },
  });
  expect(started.status(), await started.text()).toBe(200);
  const current = await sessionId(page.request, "claude-code", agentSession);
  const sessions = [
    session("ses_idle", "Setup wizard", 180, hooked("idle")),
    session(current, "Session switcher", 0, {
      ...hooked("working", { state: "working" }),
      agent_session: agentSession,
    }),
    session("ses_need", "Release notes", 12, {
      ...hooked("needs_you", {
        state: "needs_input",
        since: ago(12),
        wait: { kind: "permission", tool: "Bash", summary: "npm test" },
      }),
    }),
    session("ses_fin", "Anna short", 2, hooked("finished", { finished_at: ago(2) })),
    session("ses_stopped", "Host signing", 300, hooked("stopped")),
  ];
  await page.route(
    (url) => url.pathname === "/api/v1/sessions" && !url.searchParams.get("q"),
    (route) => route.fulfill({ json: { sessions, has_more: false } }),
  );
  return { current };
}

const palette = (page: Page) => page.getByRole("dialog", { name: "Search" });
const titles = (page: Page) =>
  palette(page)
    .getByRole("option")
    .evaluateAll((options) => options.map((o) => o.querySelector(".palette-title")?.textContent ?? ""));

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
});

test("⌘P lists the sessions that need you first, and opens on the current one", async ({ page }) => {
  const { current } = await standIn(page);
  await page.goto(`/#/sessions/claude-code/${current}`);
  await page.keyboard.press("ControlOrMeta+p");

  await expect(palette(page).getByRole("tab", { name: "Sessions" })).toHaveAttribute("aria-selected", "true");
  await expect
    .poll(() => titles(page))
    .toEqual(["Release notes", "Anna short", "Session switcherCurrent", "Earlier · 2 idle or stopped", "New session"]);
  const options = palette(page).getByRole("option");
  await expect(options.first()).toContainText("Bash · npm test");
  await expect(options.nth(1)).toContainText("Finished");
  // it opens on the current session
  await expect(options.nth(2)).toHaveAttribute("aria-selected", "true");

  // ⌘P again moves down a row, onto Earlier, which → unfolds
  await page.keyboard.press("ControlOrMeta+p");
  await expect(options.nth(3)).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowRight");
  await expect
    .poll(() => titles(page))
    .toEqual([
      "Release notes",
      "Anna short",
      "Session switcherCurrent",
      "Earlier · 2 idle or stopped",
      "Setup wizard",
      "Host signing",
      "New session",
    ]);

  // Enter goes to the selected session
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/sessions\/claude-code\/ses_need$/);
  await expect(palette(page)).toHaveCount(0);
});

test("the Sessions list ends with a New session row", async ({ page }) => {
  const { current } = await standIn(page);
  await page.goto(`/#/sessions/claude-code/${current}`);
  await page.keyboard.press("ControlOrMeta+p");
  const row = palette(page).getByRole("option", { name: /New session/ });
  await expect(row).toBeVisible();
  await row.click();
  await expect(page).toHaveURL(/\/sessions\/new$/);
});

test("⌘K lists the sessions that need you above the pending reviews", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "A pending review" });
  await standIn(page);
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+k");
  const groups = palette(page).locator(".palette-group");
  await expect(groups.first()).toHaveText("Sessions that need you");
  await expect(groups.nth(1)).toHaveText("Pending reviews");
  await expect(palette(page).getByRole("option").first()).toContainText("Release notes");
});

test("the shortcuts dialog lists the key that goes to a session", async ({ page }) => {
  await page.goto("/#/");
  await page.keyboard.press("?");
  await expect(page.getByRole("dialog", { name: "Keyboard shortcuts" })).toContainText("Go to a session");
});
