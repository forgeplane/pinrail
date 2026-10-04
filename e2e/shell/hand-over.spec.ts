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
