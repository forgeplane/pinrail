// A list whose load fails says why, rather than showing an empty page.

import { expect, test, type Page } from "@playwright/test";

test.afterEach(({ page }) => page.unrouteAll({ behavior: "ignoreErrors" }));

/** Answers the requests for `pathname` with an error the core could give. */
const failing = (page: Page, pathname: string) =>
  page.route(
    (url) => url.pathname === pathname,
    (route) => route.fulfill({ status: 500, json: { error: "internal", message: "The database is locked." } }),
  );

for (const [name, path, url] of [
  ["Sessions page", "/api/v1/sessions", "/#/sessions"],
  ["history", "/api/v1/reviews", "/#/?tab=history"],
  ["Work items page", "/api/v1/items", "/#/items"],
] as const)
  test(`the ${name} says when its list cannot be loaded`, async ({ page }) => {
    await failing(page, path);
    await page.goto(url);
    await expect(page.locator(".notice-danger")).toContainText("The database is locked.");
  });

test("the quick add says when it cannot load the repositories", async ({ page }) => {
  await page.goto("/#/");
  await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();
  await failing(page, "/api/v1/projects");
  await page.keyboard.press("ControlOrMeta+Shift+C");
  const dialog = page.getByRole("dialog", { name: "Add a work item" });
  await expect(dialog.locator(".notice-danger")).toContainText("The database is locked.");
});
