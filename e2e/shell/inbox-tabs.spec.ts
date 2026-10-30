// The inbox's tabs: Pending, New rounds and History, under one filter bar.

import { expect, test } from "@playwright/test";
import { clearInbox, core, createReview, decide } from "./helpers";

const approved = { decisions: [{ id: 1, action: "accept" }], undecided: [] };
const tab = (page: import("@playwright/test").Page, name: string) =>
  page.getByRole("tablist", { name: "Inbox views" }).getByRole("tab", { name });

test("the old History address opens the History tab with its filters", async ({ page }) => {
  await clearInbox(page.request);
  const run = Date.now();
  const review = await createReview(page.request, { title: `Tabs: redirected ${run}` });
  await decide(page.request, review.id, approved);

  await page.goto(`/#/history?q=${run}`);
  await expect(page).toHaveURL(new RegExp(`#/\\?q=${run}&tab=history$`));
  await expect(tab(page, "History")).toHaveAttribute("aria-selected", "true");
  await expect(page.getByTestId("history-row")).toHaveText([new RegExp(`Tabs: redirected ${run}`)]);
  // the sidebar has no History entry of its own
  await expect(page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: "History" })).toHaveCount(0);
});

test("switching tabs keeps the search and the project, and leaves the outcome behind", async ({ page }) => {
  await clearInbox(page.request);
  const run = Date.now();
  await createReview(page.request, { title: `Tabs: waiting ${run}` });
  const ended = await createReview(page.request, { title: `Tabs: ended ${run}` });
  await decide(page.request, ended.id, approved);

  await page.goto("/#/");
  await page.getByRole("searchbox", { name: "Search inbox" }).fill(String(run));
  await expect(page.getByTestId("review-row")).toHaveText([new RegExp(`Tabs: waiting ${run}`)]);
  // Outcome applies to the History tab only
  await expect(page.getByRole("button", { name: /Outcome/ })).toHaveCount(0);

  await tab(page, "History").click();
  await expect(page.getByRole("searchbox", { name: "Search history" })).toHaveValue(String(run));
  await expect(page.getByTestId("history-row")).toHaveText([new RegExp(`Tabs: ended ${run}`)]);
  await expect(page.getByRole("button", { name: /Outcome/ })).toBeVisible();
  // the layout applies to the waiting tabs only
  await expect(page.getByRole("radiogroup", { name: "Inbox layout" })).toHaveCount(0);

  await page.goto(`/#/?tab=history&q=${run}&status=withdrawn`);
  await expect(page.getByTestId("history-row")).toHaveCount(0);
  await tab(page, "Pending").click();
  await expect(page).toHaveURL(new RegExp(`#/\\?q=${run}$`));
  await expect(page.getByTestId("review-row")).toHaveText([new RegExp(`Tabs: waiting ${run}`)]);
});

test("the star filter lists the starred waiting reviews", async ({ page }) => {
  await clearInbox(page.request);
  const kept = await createReview(page.request, { title: "Tabs: starred and waiting" });
  await createReview(page.request, { title: "Tabs: waiting, no star" });
  expect((await page.request.put(`${core}/api/v1/reviews/${kept.id}/star`)).status()).toBe(204);

  await page.goto("/#/");
  await expect(page.getByTestId("review-row")).toHaveCount(2);
  await page.getByRole("button", { name: "Starred", exact: true }).click();
  await expect(page).toHaveURL(/starred=1/);
  await expect(page.getByTestId("review-row")).toHaveText([/Tabs: starred and waiting/]);
});

test("⌘⇧H opens the History tab", async ({ page }) => {
  await page.goto("/#/");
  await expect(tab(page, "Pending")).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press(process.platform === "darwin" ? "Meta+Shift+H" : "Control+Shift+H");
  await expect(page).toHaveURL(/#\/\?tab=history$/);
  await expect(tab(page, "History")).toHaveAttribute("aria-selected", "true");
});
