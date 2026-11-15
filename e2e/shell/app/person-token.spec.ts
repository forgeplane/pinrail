// The window acts as the person with the person token it was given: the
// browser itself sends none, only the requests of the specs do.

import { expect, test } from "../fixtures";
import { createReview } from "../helpers";

test("the window sends the person token with what the person does", async ({ page }) => {
  // an agent's submission needs no token
  const { id } = await createReview(page.request, { title: "Person token: the plan" });
  await page.goto(`/#/reviews/${id}`);
  const button = page.getByTestId("star");
  await expect(button).toHaveAttribute("aria-pressed", "false");

  // a star only the person can give, kept by the core across a reload
  await button.click();
  await expect(button).toHaveAttribute("aria-pressed", "true");
  await page.reload();
  await expect(button).toHaveAttribute("aria-pressed", "true");
});
