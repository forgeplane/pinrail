import { expect, test } from "@playwright/test";
import { clearInbox, createReview, decide, openReviewsIn } from "./helpers";

test.afterEach(({ request }) => openReviewsIn(request, "panel"));

test("a review opened from its title in the history is one step back from it", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  await clearInbox(page.request);
  // the history keeps every run's reviews, so each run's title is its own
  const title = `History: back once ${Date.now()}`;
  const review = await createReview(page.request, { title });
  await decide(page.request, review.id, { decisions: [], undecided: [1, 2] });

  await page.goto("/#/");
  await page.goto("/#/history");
  await page.getByTestId("history-row").getByRole("link", { name: title }).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${review.id}$`));
  await page.goBack();
  await expect(page).toHaveURL(/#\/history$/);
});

test("the history opens a review beside the list, steps with J and K, and closes with Escape", async ({ page }) => {
  await clearInbox(page.request);
  const run = Date.now();
  for (const title of [`Beside history first ${run}`, `Beside history second ${run}`]) {
    const review = await createReview(page.request, { title });
    await decide(page.request, review.id, { decisions: [], undecided: [1, 2] });
  }

  // only this test's reviews: the history keeps the others
  await page.goto(`/#/history?q=${run}`);
  const rows = page.getByTestId("history-row");
  // newest first
  await expect(rows).toHaveText([/Beside history second/, /Beside history first/]);
  await rows.first().getByRole("link").click();
  const panel = page.getByTestId("review-panel");
  await expect(panel).toBeVisible();
  await expect(panel).toContainText("Beside history second");
  // the list stays in view, with the open review marked
  await expect(rows.first()).toHaveAttribute("aria-current", "true");
  await expect(page).toHaveURL(new RegExp(`#/history\\?q=${run}&open=`));

  await page.locator("body").press("j");
  await expect(panel).toContainText("Beside history first");
  await expect(rows.nth(1)).toHaveAttribute("aria-current", "true");
  // at the end, J goes round to the first
  await page.locator("body").press("j");
  await expect(panel).toContainText("Beside history second");
  await page.locator("body").press("k");
  await expect(panel).toContainText("Beside history first");

  await page.locator("body").press("Escape");
  await expect(panel).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`#/history\\?q=${run}$`));
});

test("a second history row clicked before the panel shows replaces the first in the history", async ({ page }) => {
  await clearInbox(page.request);
  const run = Date.now();
  for (const title of [`Quick history first ${run}`, `Quick history second ${run}`]) {
    const review = await createReview(page.request, { title });
    await decide(page.request, review.id, { decisions: [], undecided: [1, 2] });
  }

  await page.goto("/#/");
  await page.goto(`/#/history?q=${run}`);
  const rows = page.getByTestId("history-row");
  await expect(rows).toHaveCount(2);
  // the title of one and the row of the other, in one task, so the screen cannot render between them
  await rows.first().evaluate((first) => {
    first.querySelector<HTMLElement>("a")!.click();
    (first.nextElementSibling as HTMLElement).click();
  });
  await expect(page.getByTestId("review-panel")).toContainText("Quick history first");
  await page.goBack();
  await expect(page).toHaveURL(new RegExp(`#/history\\?q=${run}$`));
});

test("J and K beside the history move the highlight, not the row a scroll brings under the pointer", async ({
  page,
}) => {
  await clearInbox(page.request);
  const run = Date.now();
  for (let n = 1; n <= 20; n++) {
    const review = await createReview(page.request, { title: `Still pointer ${n} ${run}` });
    await decide(page.request, review.id, { decisions: [], undecided: [1, 2] });
  }
  await page.setViewportSize({ width: 1200, height: 520 });
  await page.goto(`/#/history?q=${run}&per=25`);
  const rows = page.getByTestId("history-row");
  await expect(rows).toHaveCount(20);
  await rows.first().getByRole("link").click();
  await expect(page.getByTestId("review-panel")).toBeVisible();
  // the pointer rests on a row near the top
  await rows.nth(2).hover();

  // K from the first goes round to the last, and the list scrolls to it under the still pointer
  await page.locator("body").press("k");
  await expect(rows.nth(19)).toHaveAttribute("aria-current", "true");
  await page.waitForTimeout(300);
  await expect(page.locator(".history-table tr.is-focused")).toHaveCount(1);
  await expect(rows.nth(19)).toHaveClass(/is-focused/);
});
