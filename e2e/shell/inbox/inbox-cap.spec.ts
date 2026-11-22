import { expect, test } from "../fixtures";
import { clearInbox, createReview } from "../helpers";

// a route handler may still be reading a response when the test ends, and
// closing the page disposes it: such a handler's error is not the test's
test.afterEach(({ page }) => page.unrouteAll({ behavior: "ignoreErrors" }));

/** All lists the first 200 notifications that wait; beyond that it says how
 *  many there are and leads to the tab that holds them all. */
test("All says how many wait when it shows only some, and leads to Waiting on you", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Capped: one that waits" });
  await page.route(
    (url) => url.pathname === "/api/v1/notifications" && url.searchParams.get("tab") === "waiting",
    async (route) => {
      const response = await route.fetch();
      const listing = await response.json();
      await route.fulfill({ response, json: { ...listing, total: 700 } });
    },
  );
  await page.goto("/");
  const section = page.getByRole("region", { name: "Waiting on you" });
  await expect(section.getByRole("heading")).toContainText("700");
  const more = section.getByTestId("inbox-waiting-more");
  await expect(more).toHaveText("1 shown · see all");
  await more.click();
  await expect(page).toHaveURL(/tab=waiting/);
});
