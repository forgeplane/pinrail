import { expect, test, type APIRequestContext } from "@playwright/test";
import { clearInbox, core, createReview, decide } from "./helpers";

const payload = {
  summary: "Two proposals.",
  groups: [
    {
      title: "lib/acme/tickets.ex",
      items: [
        { id: 1, severity: "major", title: "do_save dedups without reversing" },
        { id: 2, severity: "minor", title: "moduledoc typo" },
      ],
    },
  ],
};

/** A list review from `workflow`, in acme/api unless told otherwise. */
function review(request: APIRequestContext, title: string, workflow: string, repo: string | null = "acme/api") {
  return createReview(request, { title, origin: repo ? { repo, workflow } : { workflow }, payload });
}

/** Leaves both proposals undecided, which decides the review. */
const decideNothing = (request: APIRequestContext, id: string) =>
  decide(request, id, { decisions: [], undecided: [1, 2] });

test("the sidebar lists what is waiting on every page, oldest first, and ⌥↓ walks it", async ({ page }) => {
  await clearInbox(page.request);
  const first = await review(page.request, "Sidebar: the older one", "pr-review");
  const second = await review(page.request, "Sidebar: the newer one", "triage");

  const loose = await review(page.request, "Sidebar: no project", "cron", null);

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
  await decideNothing(page.request, loose.id);
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
  await decideNothing(page.request, second.id);
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText("the older one");
  await decideNothing(page.request, first.id);
  await expect(waiting).toHaveCount(0);
});

test("more than five waiting points at the inbox", async ({ page }) => {
  await clearInbox(page.request);
  const ids: string[] = [];
  for (let i = 1; i <= 7; i++) ids.push((await review(page.request, `Sidebar: batch ${i}`, "batch")).id);
  await page.goto("/#/history");
  const waiting = page.locator("[data-waiting]");
  await expect(waiting.locator("[data-waiting-review]")).toHaveCount(5);
  await expect(waiting.locator(".sidebar-more")).toHaveText("2 more in the inbox");
  for (const id of ids) await decideNothing(page.request, id);
  await expect(waiting).toHaveCount(0);
});

test("⌘B hides the sidebar and shows it again", async ({ page }) => {
  await page.goto("/#/history");
  const toggle = page.locator("button.bar-button[aria-label$='sidebar']");
  await expect(toggle).toHaveAttribute("aria-label", "Hide sidebar");

  await page.keyboard.press("ControlOrMeta+b");
  await expect(toggle).toHaveAttribute("aria-label", "Show sidebar");
  await page.keyboard.press("ControlOrMeta+b");
  await expect(toggle).toHaveAttribute("aria-label", "Hide sidebar");
});

test("a quick second ⌘B is not undone by the reload the first one caused", async ({ page }) => {
  await page.goto("/#/history");
  const toggle = page.locator("button.bar-button[aria-label$='sidebar']");
  await expect(toggle).toHaveAttribute("aria-label", "Hide sidebar");
  // a slow server: the settings the first change makes the app reload come
  // back after the second change
  await page.route(`${core}/api/v1/settings`, async (route) => {
    if (route.request().method() !== "GET") return route.continue();
    const response = await route.fetch();
    await new Promise((r) => setTimeout(r, 800));
    await route.fulfill({ response });
  });

  await page.keyboard.press("ControlOrMeta+b");
  await expect(toggle).toHaveAttribute("aria-label", "Show sidebar");
  await page.waitForTimeout(200);
  await page.keyboard.press("ControlOrMeta+b");
  await expect(toggle).toHaveAttribute("aria-label", "Hide sidebar");
  // and it stays shown while every reload comes back
  const seen = new Set<string | null>();
  for (let i = 0; i < 40; i++) {
    seen.add(await toggle.getAttribute("aria-label"));
    await page.waitForTimeout(50);
  }
  expect([...seen]).toEqual(["Hide sidebar"]);
  await page.unroute(`${core}/api/v1/settings`);
});

test("the app's own text cannot be selected, as in a desktop app, but fields and messages can", async ({ page }) => {
  await page.goto("/");
  const selectable = (selector: string) =>
    page
      .locator(selector)
      .first()
      .evaluate((el) => getComputedStyle(el).webkitUserSelect || getComputedStyle(el).userSelect);
  await expect(page.locator(".nav-count").first()).toBeVisible();
  expect(await selectable(".nav-count")).toBe("none");
  expect(await selectable("h1")).toBe("none");
  expect(await selectable("input[type=search]")).not.toBe("none");

  // a review's title and id are there to be copied
  const review = await createReview(page.request, { title: "Copy my title", payload });
  await page.goto(`/#/reviews/${review.id}`);
  await expect(page.locator(".crumb-title")).toHaveText("Copy my title");
  expect(await selectable(".crumb-title")).not.toBe("none");
  expect(await selectable(".strip-id .mono")).not.toBe("none");
});
