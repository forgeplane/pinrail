import { expect, test } from "@playwright/test";
import { clearInbox, createReview, decide } from "./helpers";

test("a review opened from its title in the history is one step back from it", async ({ page }) => {
  await clearInbox(page.request);
  const review = await createReview(page.request, { title: "History: back once" });
  await decide(page.request, review.id, { decisions: [], undecided: [1, 2] });

  await page.goto("/#/");
  await page.goto("/#/history");
  await page.locator(".history-title", { hasText: "History: back once" }).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${review.id}$`));
  await page.goBack();
  await expect(page).toHaveURL(/#\/history$/);
});
