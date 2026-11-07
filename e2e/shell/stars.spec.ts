// Stars the person gives a review, so they can find it again: from the
// review's toolbar and its shortcut, from a row of the inbox and of the
// history, and with the history's filter.

import { expect, test, type APIRequestContext } from "@playwright/test";
import { clearInbox, core, createReview, decide, onePayload, openReviewsIn, toggleFilter } from "./helpers";

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
  await toggleFilter(page, "Starred");
  await expect(page).toHaveURL(/starred=1/);
  await expect(rows.filter({ hasText: "Stars: kept" })).toHaveCount(1);
  await expect(rows.filter({ hasText: "Stars: not kept" })).toHaveCount(0);

  // the row's menu removes the star, and the filtered list lets it go
  const keptRow = rows.filter({ hasText: "Stars: kept" });
  await expect(keptRow.getByLabel("Starred")).toBeVisible();
  await keptRow.getByRole("button", { name: "Stars: kept actions" }).click();
  await page.getByRole("menu", { name: "Stars: kept" }).getByRole("menuitem", { name: "Remove star" }).click();
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

test("a session's details list its starred reviews and mark them", async ({ page }) => {
  await clearInbox(page.request);
  const session = `s-stars-${Date.now()}`;
  const sent = async (title: string) => {
    const response = await page.request.post(`${core}/api/v1/reviews`, {
      data: {
        plugin: "list",
        title,
        origin: { repo: "acme/api" },
        requested_by: "claude-code",
        session,
        payload: onePayload,
      },
    });
    expect(response.status()).toBe(201);
    return ((await response.json()) as { id: string }).id;
  };
  const waiting = await sent("Stars: still waiting");
  const ended = await sent("Stars: decided");
  await sent("Stars: not starred");
  await decide(page.request, ended, approved);
  await star(page.request, waiting);
  await star(page.request, ended);

  await page.goto(`/#/sessions/claude-code/${session}`);
  const details = page.getByRole("complementary", { name: "Session details" });
  const listed = details.getByRole("region", { name: "Starred" }).getByTestId("starred-review");
  // newest first
  await expect(listed).toHaveText([/Stars: decided/, /Stars: still waiting/]);
  // what waits on you shows its star
  const item = details.getByTestId("waiting-item").filter({ hasText: "Stars: still waiting" });
  await expect(item.getByRole("img", { name: "Starred" })).toBeVisible();
  await expect(
    details.getByTestId("waiting-item").filter({ hasText: "Stars: not starred" }).getByRole("img", { name: "Starred" }),
  ).toHaveCount(0);

  // a star removed elsewhere leaves the list at once
  const response = await page.request.delete(`${core}/api/v1/reviews/${waiting}/star`);
  expect(response.status()).toBe(204);
  await expect(listed).toHaveText([/Stars: decided/]);
  await expect(item.getByRole("img", { name: "Starred" })).toHaveCount(0);
});

test("the history's star filter lists only the starred round of a review", async ({ page }) => {
  await clearInbox(page.request);
  const first = await createReview(page.request, { title: "Stars: rounds one" });
  const second = await createReview(page.request, { title: "Stars: rounds two", revises: first.id });
  await decide(page.request, second.id, approved);
  await star(page.request, first.id);

  await page.goto("/#/history?starred=1");
  const rows = page.getByTestId("history-row");
  await expect(rows.filter({ hasText: "Stars: rounds one" })).toHaveCount(1);
  await expect(rows.filter({ hasText: "Stars: rounds two" })).toHaveCount(0);
});
