// Stars the person gives a review, so they can find it again: from the
// review's toolbar and its shortcut, from a row of the inbox and of the
// history, and with the history's filter.

import { expect, test, type APIRequestContext } from "@playwright/test";
import { clearInbox, core, createReview, decide, openReviewsIn } from "./helpers";

const approved = { decisions: [{ id: 1, action: "accept" }], undecided: [] };

async function star(request: APIRequestContext, id: string) {
  const response = await request.put(`${core}/api/v1/reviews/${id}/star`);
  expect(response.status()).toBe(204);
}

test.afterEach(({ request }) => openReviewsIn(request, "panel"));

test("a review is starred from its toolbar and with ⌘⇧S", async ({ page }) => {
  await clearInbox(page.request);
  await openReviewsIn(page.request, "page");
  const { id } = await createReview(page.request, { title: "Stars: the plan" });
  await page.goto(`/#/reviews/${id}`);
  const button = page.getByTestId("star");

  await expect(button).toHaveAttribute("aria-pressed", "false");
  await button.click();
  await expect(button).toHaveAttribute("aria-pressed", "true");
  await expect(button).toHaveAccessibleName("Remove the star");
  // kept by the core, so a reload shows it
  await page.reload();
  await expect(button).toHaveAttribute("aria-pressed", "true");

  await page.keyboard.press(process.platform === "darwin" ? "Meta+Shift+S" : "Control+Shift+S");
  await expect(button).toHaveAttribute("aria-pressed", "false");
});

test("an inbox row stars its review without opening it", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Stars: from the row" });
  await page.goto("/#/");
  const row = page.getByTestId("review-row").filter({ hasText: "Stars: from the row" });
  await row.hover();
  await row.getByTestId("star").click();
  await expect(row.getByTestId("star")).toHaveClass(/is-starred/);
  await expect(page.getByTestId("review-panel")).toHaveCount(0);
  // a starred row shows its star without the pointer over it
  await page.mouse.move(0, 0);
  await expect(row.getByTestId("star")).toBeVisible();
});

test("the history lists the starred reviews with its filter", async ({ page }) => {
  await clearInbox(page.request);
  const kept = await createReview(page.request, { title: "Stars: kept" });
  const other = await createReview(page.request, { title: "Stars: not kept" });
  await decide(page.request, kept.id, approved);
  await decide(page.request, other.id, approved);
  await star(page.request, kept.id);

  await page.goto("/#/history");
  const rows = page.getByTestId("history-row");
  await expect(rows.filter({ hasText: "Stars: not kept" })).toHaveCount(1);
  await page.getByRole("button", { name: "Starred" }).click();
  await expect(page).toHaveURL(/starred=1/);
  await expect(rows.filter({ hasText: "Stars: kept" })).toHaveCount(1);
  await expect(rows.filter({ hasText: "Stars: not kept" })).toHaveCount(0);

  // the row's star removes it, and the filtered list lets it go
  await rows.filter({ hasText: "Stars: kept" }).getByTestId("star").click();
  await expect(rows.filter({ hasText: "Stars: kept" })).toHaveCount(0);
});

test("a star given elsewhere shows at once, and marks every round", async ({ page }) => {
  await clearInbox(page.request);
  await openReviewsIn(page.request, "page");
  const first = await createReview(page.request, { title: "Stars: round one" });
  const second = await createReview(page.request, { title: "Stars: round two", revises: first.id });
  await page.goto(`/#/reviews/${second.id}`);
  const button = page.getByTestId("star");
  await expect(button).toHaveAccessibleName("Star");

  // another window stars the first round
  await star(page.request, first.id);
  await expect(button).toHaveAccessibleName("Star this round; another round is starred");
  // the inbox shows the review starred through its earlier round
  await page.goto("/#/");
  const row = page.getByTestId("review-row").filter({ hasText: "Stars: round two" });
  await expect(row.getByTestId("star")).toHaveClass(/is-starred/);
  await expect(row.getByTestId("star")).toHaveAccessibleName("An earlier round is starred");
});
