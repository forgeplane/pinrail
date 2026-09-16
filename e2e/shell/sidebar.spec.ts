import { expect, test, type APIRequestContext } from "@playwright/test";

const core = "http://127.0.0.1:4799";

const payload = {
  intro: "Two proposals.",
  allow_additions: false,
  groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "major", title: "do_save dedups without reversing" }, { id: 2, severity: "minor", title: "moduledoc typo" }] }],
};

async function createReview(request: APIRequestContext, title: string, workflow: string, repo: string | null = "acme/api") {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: { plugin: "list", title, origin: repo ? { repo, workflow } : { workflow }, requested_by: "spec", payload },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { id: string };
}

/** Discards whatever is pending, so a test starts from an empty inbox. */
async function clearInbox(request: APIRequestContext) {
  const pending = (await (await request.get(`${core}/api/v1/reviews?status=pending`)).json()) as { id: string }[];
  for (const r of pending) await request.post(`${core}/api/v1/reviews/${r.id}/discard`, { data: { reason: "spec cleanup" } });
}

async function decide(request: APIRequestContext, id: string) {
  const response = await request.post(`${core}/api/v1/reviews/${id}/decision`, { data: { data: { decisions: [], undecided: [1, 2] } } });
  expect(response.status()).toBe(200);
}

test("the sidebar lists what is waiting on every page, oldest first, and ⌥↓ walks it", async ({ page }) => {
  await clearInbox(page.request);
  const first = await createReview(page.request, "Sidebar: the older one", "pr-review");
  const second = await createReview(page.request, "Sidebar: the newer one", "triage");

  const loose = await createReview(page.request, "Sidebar: no project", "cron", null);

  // on the history page, not the inbox: what waits is listed, and the
  // projects with their counts, the same as everywhere else; a review
  // that names no project has a row of its own
  await page.goto("/#/history");
  const waiting = page.locator("[data-waiting]");
  await expect(waiting).toBeVisible();
  await expect(page.locator('[data-project="acme/api"]')).toContainText("2");
  await expect(page.locator('[data-project="-"]')).toContainText("1");
  await page.locator('[data-project="-"]').click();
  await expect(page).toHaveURL(/repo=-/);
  await expect(page.locator(".inbox-repo summary strong")).toHaveText(["No project"]);
  await expect(page.locator("#inbox-repo")).toContainText("No project");
  await decide(page.request, loose.id);
  await expect(page.locator('[data-project="-"]')).toHaveCount(0);
  await page.goto("/#/history");
  const rows = waiting.locator("[data-waiting-review]");
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(0)).toContainText("the older one");
  await expect(rows.nth(1)).toContainText("the newer one");

  // a click opens the review and the row reads as current
  await rows.nth(1).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${second.id}$`));
  await expect(rows.nth(1)).toHaveClass(/active/);

  // ⌥↓ moves to the next waiting review, wrapping; ⌥↑ goes back. The
  // keys are the shell's, so the view's frame must not hold the focus.
  await page.keyboard.press("Alt+ArrowDown");
  await expect(page).toHaveURL(new RegExp(`/reviews/${first.id}$`));
  await page.locator(".app-topbar").click();
  await page.keyboard.press("Alt+ArrowUp");
  await expect(page).toHaveURL(new RegExp(`/reviews/${second.id}$`));

  // still there inside settings
  await page.keyboard.press("ControlOrMeta+,");
  await expect(page.locator("[data-settings]")).toBeVisible();
  await expect(waiting).toBeVisible();
  await page.keyboard.press("Escape");

  // a decision takes its row away; the group goes with the last one
  await decide(page.request, second.id);
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText("the older one");
  await decide(page.request, first.id);
  await expect(waiting).toHaveCount(0);
});

test("more than five waiting points at the inbox", async ({ page }) => {
  await clearInbox(page.request);
  const ids: string[] = [];
  for (let i = 1; i <= 7; i++) ids.push((await createReview(page.request, `Sidebar: batch ${i}`, "batch")).id);
  await page.goto("/#/history");
  const waiting = page.locator("[data-waiting]");
  await expect(waiting.locator("[data-waiting-review]")).toHaveCount(5);
  await expect(waiting.locator(".sidebar-more")).toHaveText("2 more in the inbox");
  for (const id of ids) await decide(page.request, id);
  await expect(waiting).toHaveCount(0);
});
