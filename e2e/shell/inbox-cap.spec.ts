import { expect, test } from "@playwright/test";

/** The inbox loads at most 500 pending reviews; beyond that it says so. */
test("an inbox with more pending reviews than it loads says how many there are", async ({ page }) => {
  await page.route(
    (url) => url.pathname === "/api/v1/reviews" && url.searchParams.get("status") === "pending",
    async (route) => {
      const response = await route.fetch();
      const listing = await response.json();
      await route.fulfill({ response, json: { ...listing, total: 700, has_more: true } });
    },
  );
  await page.goto("/");
  await expect(page.locator("[data-inbox-capped]")).toContainText("700");
  await expect(page.locator(".nav-count").first()).toHaveText("700");
});
