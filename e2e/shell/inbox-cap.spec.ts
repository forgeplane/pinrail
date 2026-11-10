import { expect, test } from "@playwright/test";

// a route handler may still be reading a response when the test ends, and
// closing the page disposes it: such a handler's error is not the test's
test.afterEach(({ page }) => page.unrouteAll({ behavior: "ignoreErrors" }));

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
  await expect(page.getByTestId("inbox-capped")).toContainText("700");
  await expect(page.getByTestId("inbox-count")).toHaveText("700");
});
