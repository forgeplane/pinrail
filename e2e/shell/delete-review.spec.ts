import { expect, test } from "@playwright/test";
import { core, createReview, decide, openReviewsIn } from "./helpers";

const approved = { decisions: [{ id: 1, action: "accept" }], undecided: [] };

// one test opens reviews as pages; the others in the shard expect the panel
test.afterEach(({ request }) => openReviewsIn(request, "panel"));

test("the history's menu deletes a review with every round of it", async ({ page, request }) => {
  const run = Date.now();
  const first = await createReview(request, { title: `Delete: round one ${run}` });
  const second = await createReview(request, { title: `Delete: round two ${run}`, revises: first.id });
  await decide(request, second.id, approved);
  const kept = await createReview(request, { title: `Delete: kept ${run}` });
  await decide(request, kept.id, approved);

  await page.goto(`/#/?tab=history&q=${run}`);
  const rows = page.getByTestId("history-row");
  await expect(rows).toHaveCount(3);
  const title = `Delete: round two ${run}`;
  await rows
    .filter({ hasText: title })
    .getByRole("button", { name: `${title} actions` })
    .click();
  await page.getByRole("menu", { name: title }).getByRole("menuitem", { name: "Delete review…" }).click();
  const dialog = page.getByRole("dialog", { name: "Delete this review?" });
  await expect(dialog.getByTestId("delete-review-waiting")).toHaveCount(0);
  await dialog.getByRole("button", { name: "Delete review" }).click();

  // both rounds go, and the other review stays
  await expect(rows).toHaveText([new RegExp(`Delete: kept ${run}`)]);
  for (const id of [first.id, second.id]) {
    expect((await request.get(`${core}/api/v1/reviews/${id}`)).status()).toBe(404);
  }
});

test("a review's screen deletes a waiting review, which is discarded first", async ({ page, request }) => {
  await openReviewsIn(request, "page");
  const run = Date.now();
  const { id } = await createReview(request, { title: `Delete: waiting ${run}` });
  await page.goto(`/#/reviews/${id}`);
  await page.getByRole("button", { name: "Delete this review" }).click();
  const dialog = page.getByRole("dialog", { name: "Delete this review?" });
  await expect(dialog.getByTestId("delete-review-waiting")).toBeVisible();
  await dialog.getByRole("button", { name: "Delete review" }).click();

  // the screen returns to the inbox, where the review no longer waits
  await expect(page).toHaveURL(/#\/$/);
  await expect(page.getByTestId("review-row").filter({ hasText: `Delete: waiting ${run}` })).toHaveCount(0);
  expect((await request.get(`${core}/api/v1/reviews/${id}`)).status()).toBe(404);
});

test("the history row shows a starred round's star, and its menu stars it", async ({ page, request }) => {
  const run = Date.now();
  const { id } = await createReview(request, { title: `Delete: star ${run}` });
  await decide(request, id, approved);
  await page.goto(`/#/?tab=history&q=${run}`);
  const row = page.getByTestId("history-row");
  await expect(row.getByLabel("Starred")).toHaveCount(0);
  await row.getByRole("button", { name: `Delete: star ${run} actions` }).click();
  await page.getByRole("menuitem", { name: "Star", exact: true }).click();
  await expect(row.getByLabel("Starred")).toBeVisible();
  // the click stayed off the row: nothing opened
  await expect(page).toHaveURL(/tab=history/);
  await expect(page).not.toHaveURL(/reviews\//);
});
