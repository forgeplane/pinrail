import { expect, test } from "@playwright/test";
import { clearInbox, createReview } from "./helpers";

test("handing over a decision returns to the inbox", async ({ page }) => {
  await clearInbox(page.request);
  const review = await createReview(page.request, { title: "Hand over and go back" });
  await createReview(page.request, { title: "Still waiting" });

  await page.goto("/#/");
  await page.locator("[data-review-row]", { hasText: "Hand over and go back" }).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${review.id}$`));
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator("body")).toContainText("One proposal.");
  // the person decides the proposal and hands over
  await frame.locator('button[data-act="accept"]').first().click();
  await page.locator("[data-handover]").click();

  await expect(page).toHaveURL(/#\/$/);
  await expect(page.getByText("Decision recorded")).toBeVisible();
  const rows = page.locator("[data-review-row]");
  await expect(rows).toHaveCount(1);
  await expect(rows).toContainText("Still waiting");
});

/** A list review with `count` items: a short view, or a tall one. */
const listOf = (title: string, count: number) => ({
  plugin: "list",
  title,
  payload: {
    intro: `${count} proposals.`,
    groups: [
      {
        title: "lib/acme/tickets.ex",
        items: Array.from({ length: count }, (_, i) => ({ id: i + 1, severity: "minor", title: `item ${i + 1}` })),
      },
    ],
  },
});

test("the frame fills the panel: a short view's hand-over sits where a tall one's does", async ({ page }) => {
  await clearInbox(page.request);
  const short = await createReview(page.request, listOf("Short view", 1));
  const tall = await createReview(page.request, listOf("Tall view", 60));
  const handover = page.locator("[data-handover]");
  const frame = page.frameLocator("#plugin-frame");

  await page.goto(`/#/reviews/${short.id}`);
  await expect(frame.locator("body")).toContainText("1 proposals.");
  const shortFrame = (await page.locator("#plugin-frame").boundingBox())!;
  const shortButton = (await handover.boundingBox())!;

  await page.goto(`/#/reviews/${tall.id}`);
  await expect(frame.locator("body")).toContainText("60 proposals.");
  const tallFrame = (await page.locator("#plugin-frame").boundingBox())!;
  const tallButton = (await handover.boundingBox())!;

  expect(Math.round(tallFrame.height)).toBe(Math.round(shortFrame.height));
  expect(Math.round(tallButton.y)).toBe(Math.round(shortButton.y));
});

test("a tall view scrolls inside its frame, and its header stays in place", async ({ page }) => {
  await clearInbox(page.request);
  const tall = await createReview(page.request, listOf("Tall and scrolled", 60));
  await page.goto(`/#/reviews/${tall.id}`);
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator("body")).toContainText("60 proposals.");

  const header = frame.locator(".plugin-header");
  const before = (await header.boundingBox())!;
  const last = frame.getByText("item 60", { exact: true });
  await expect(last).not.toBeInViewport();
  await last.scrollIntoViewIfNeeded();
  await expect(last).toBeInViewport();
  const after = (await header.boundingBox())!;
  expect(Math.round(after.y)).toBe(Math.round(before.y));
  // the page itself did not scroll: the view did, inside the frame
  expect(await page.evaluate(() => document.scrollingElement!.scrollTop)).toBe(0);
});
