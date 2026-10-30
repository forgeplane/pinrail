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
  await expect(page.getByTestId("inbox-total")).toHaveText("0");
  const fetched = requests(page, pendingList);

  const review = await createReview(page.request, { title: "Live: arrives" });
  const row = page.getByTestId("review-row").filter({ hasText: "Live: arrives" });
  await expect(row).toBeVisible();
  await decide(page.request, review.id, decideNothing);
  await expect(row).toHaveCount(0);
  expect(fetched).toEqual([]);
});

test("a review created while the pending list is loading stays in the inbox", async ({ page }) => {
  await clearInbox(page.request);
  // Delay every pending-list response: fetch it from the core at once,
  // before the review exists, and pass it to the app only after the
  // review's event has arrived.
  const held: Array<() => Promise<void>> = [];
  await page.route(
    (url) => url.origin === core && pendingList.test(url.pathname + url.search),
    async (route) => {
      const response = await route.fetch();
      const body = await response.body();
      held.push(() => route.fulfill({ response, body }));
    },
  );
  await page.goto("/#/");
  // Wait until the event stream is connected and its request for the
  // pending list is being held.
  await expect(page.getByText("Connected", { exact: true })).toBeVisible();
  await expect.poll(() => held.length).toBeGreaterThanOrEqual(2);

  await createReview(page.request, { title: "Live: arrives mid-fetch" });
  const row = page.getByTestId("review-row").filter({ hasText: "Live: arrives mid-fetch" });
  await expect(row).toBeVisible();

  for (const release of held.splice(0)) await release();
  await expect(page.getByTestId("inbox-total")).toHaveText("1");
  await page.waitForTimeout(300);
  await expect(row).toBeVisible();
});

test("the history and the Plugins section fetch again only for what they show", async ({ page }) => {
  await clearInbox(page.request);
  await page.goto("/#/?tab=history");
  await expect(page.getByRole("tab", { name: "History", selected: true })).toBeVisible();
  const history = requests(page, /^\/api\/v1\/reviews\?.*status=decided/);

  // a review that arrives is not in the history
  const review = await createReview(page.request, { title: "Live: not history yet" });
  await expect(page.getByTestId("waiting-review").filter({ hasText: "Live: not history yet" })).toBeVisible();
  expect(history).toEqual([]);
  // once it ends, it is
  await decide(page.request, review.id, decideNothing);
  await expect(page.getByTestId("history-row").filter({ hasText: "Live: not history yet" })).toBeVisible();

  await page.goto("/#/plugins");
  await expect(page.getByTestId("plugin-row-list")).toBeVisible();
  const plugins = requests(page, /^\/api\/v1\/plugins$/);
  await createReview(page.request, { title: "Live: not a plugin" });
  await expect(page.getByTestId("waiting-review").filter({ hasText: "Live: not a plugin" })).toBeVisible();
  expect(plugins).toEqual([]);
});
