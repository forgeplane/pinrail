// The breadcrumb of a review names where it was opened from: a review opened
// from a session's thread or from under a session in the sidebar names the
// session, and its decision leads back to the thread.

import { expect, test, type Page } from "@playwright/test";
import { core, onePayload, onReviewPages } from "./helpers";

// these tests open a review on a page of its own, which the breadcrumb heads
onReviewPages();

let n = 0;
/** A session of its own for each test, so threads never share reviews. */
const fresh = () => `s-crumb-${Date.now()}-${++n}`;
const crumb = (page: Page) => page.getByTestId("top-bar").locator(".crumb");

async function sessionWithReview(page: Page, title: string, stage = "Tidy the sidebar") {
  const session = fresh();
  const status = await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { kind: "start", text: stage },
  });
  expect(status.status(), await status.text()).toBe(200);
  const response = await page.request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "list",
      title,
      origin: { repo: `acme/${session}` },
      requested_by: "claude-code",
      session,
      payload: onePayload,
    },
  });
  expect(response.status(), await response.text()).toBe(201);
  const { id } = (await response.json()) as { id: string };
  return { session, id };
}

test("a review opened from a session's thread names the session, and its link leads back", async ({ page }) => {
  const { session, id } = await sessionWithReview(page, "Crumb: from the thread");
  await page.goto(`/#/sessions/claude-code/${session}`);
  await page.getByTestId("thread-review").click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${id}$`));
  await expect(crumb(page)).toHaveText(/^Sessions›Tidy the sidebar›Crumb: from the thread/);
  // a reload keeps it
  await page.reload();
  await expect(crumb(page)).toHaveText(/^Sessions›Tidy the sidebar›/);
  await crumb(page).getByRole("link", { name: "Tidy the sidebar" }).click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}$`));
  await expect(page.getByTestId("thread-review")).toBeVisible();
});

test("the same review opened by its address names the Inbox", async ({ page }) => {
  const { id } = await sessionWithReview(page, "Crumb: by its address");
  await page.goto(`/#/reviews/${id}`);
  await expect(crumb(page)).toHaveText(/^Inbox›Crumb: by its address/);
});

test("a review opened from under its session in the sidebar names the session", async ({ page }) => {
  const { session } = await sessionWithReview(page, "Crumb: from the sidebar", "Sort the menu");
  await page.goto("/#/?tab=history");
  const sidebar = page.getByRole("region", { name: "Repositories" });
  await sidebar.getByTestId("waiting-review").filter({ hasText: "Crumb: from the sidebar" }).click();
  await expect(crumb(page)).toHaveText(/^Sessions›Sort the menu›Crumb: from the sidebar/);
  await expect(crumb(page).getByRole("link", { name: "Sort the menu" })).toHaveAttribute(
    "href",
    `#/sessions/claude-code/${session}`,
  );
});

test("discarding a review opened from a session goes back to its thread", async ({ page }) => {
  const { session } = await sessionWithReview(page, "Crumb: discarded");
  await page.goto(`/#/sessions/claude-code/${session}`);
  await page.getByTestId("thread-review").click();
  await expect(crumb(page)).toContainText("Crumb: discarded");
  // a review opened from a session is not offered in the inbox's side panel
  await expect(page.getByRole("button", { name: "Show in the inbox's panel" })).toHaveCount(0);
  await page.getByRole("button", { name: "Discard this review" }).click();
  await page.getByRole("button", { name: "Discard", exact: true }).click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}$`));
});

test("a long session name shortens and keeps the review's title in view", async ({ page }) => {
  const { session } = await sessionWithReview(
    page,
    "Crumb: a long one",
    "Rework how the sidebar lists the sessions of every repository, grouped and ordered as the person chose",
  );
  await page.setViewportSize({ width: 1000, height: 700 });
  await page.goto(`/#/sessions/claude-code/${session}`);
  await page.getByTestId("thread-review").click();
  const name = crumb(page).locator(".crumb-session");
  await expect(name).toHaveAttribute("title", /^Rework how the sidebar/);
  expect((await name.boundingBox())?.width ?? 999).toBeLessThanOrEqual(170);
  await expect(crumb(page).getByTestId("review-title")).toBeInViewport();
  expect((await crumb(page).getByTestId("review-title").boundingBox())?.width ?? 0).toBeGreaterThan(80);
});
