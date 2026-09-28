import { expect, test, type Page } from "@playwright/test";
import { clearInbox, core, createReview, decide } from "./helpers";

/** The shell's requests to the core from now on whose path and query match. */
function requests(page: Page, pattern: RegExp): string[] {
  const seen: string[] = [];
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (request.method() === "GET" && url.origin === core && pattern.test(url.pathname + url.search)) {
      seen.push(url.pathname + url.search);
    }
  });
  return seen;
}

const pendingList = /^\/api\/v1\/reviews\?.*status=pending/;
const decideNothing = { decisions: [], undecided: [1, 2] };

test("a review that arrives or ends updates the inbox without fetching the pending list again", async ({ page }) => {
  await clearInbox(page.request);
  await page.goto("/#/");
  await expect(page.locator(".inbox-total")).toHaveText("0");
  const fetched = requests(page, pendingList);

  const review = await createReview(page.request, { title: "Live: arrives" });
  const row = page.locator("[data-review-row]", { hasText: "Live: arrives" });
  await expect(row).toBeVisible();
  await decide(page.request, review.id, decideNothing);
  await expect(row).toHaveCount(0);
  expect(fetched).toEqual([]);
});

test("the history and the Plugins section fetch again only for what they show", async ({ page }) => {
  await clearInbox(page.request);
  await page.goto("/#/history");
  await expect(page.locator(".history")).toBeVisible();
  const history = requests(page, /^\/api\/v1\/reviews\?.*status=decided/);

  // a review that arrives is not in the history
  const review = await createReview(page.request, { title: "Live: not history yet" });
  await expect(page.locator("[data-waiting-review]", { hasText: "Live: not history yet" })).toBeVisible();
  expect(history).toEqual([]);
  // once it ends, it is
  await decide(page.request, review.id, decideNothing);
  await expect(page.locator(".history-title", { hasText: "Live: not history yet" })).toBeVisible();

  await page.goto("/#/plugins");
  await expect(page.locator('[data-plugin-row="list"]')).toBeVisible();
  const plugins = requests(page, /^\/api\/v1\/plugins$/);
  await createReview(page.request, { title: "Live: not a plugin" });
  await expect(page.locator("[data-waiting-review]", { hasText: "Live: not a plugin" })).toBeVisible();
  expect(plugins).toEqual([]);
});
