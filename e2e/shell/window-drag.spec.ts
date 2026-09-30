import { expect, test } from "@playwright/test";
import { createReview } from "./helpers";

/** Tauri moves the window when the element pressed carries the drag region
 *  attribute itself, so the bar's empty space must be such an element. */
async function dragsAt(page: import("@playwright/test").Page, x: number, y: number) {
  return page.evaluate(
    ([x, y]) => document.elementFromPoint(x, y)?.hasAttribute("data-tauri-drag-region") ?? false,
    [x, y],
  );
}

test("the top bar's empty space moves the window on every screen", async ({ page }) => {
  await page.goto("/");
  const bar = page.locator(".app-topbar");
  await expect(bar).toBeVisible();
  let box = (await bar.boundingBox())!;
  expect(await dragsAt(page, box.x + box.width / 2, box.y + box.height / 2), "the inbox").toBe(true);

  // a review's bar holds its breadcrumb, which stretches across the middle
  const review = await createReview(page.request, { title: "Drag me" });
  await page.goto(`/#/reviews/${review.id}`);
  await expect(page.locator(".crumb-title")).toHaveText("Drag me");
  box = (await bar.boundingBox())!;
  expect(await dragsAt(page, box.x + box.width / 2, box.y + box.height / 2), "a review").toBe(true);
});
