// A screen that throws while it renders: the app shows what failed in its
// place, and the sidebar and the other screens stay usable.

import { expect, test } from "@playwright/test";

test("a screen that fails to render leaves the rest of the app usable", async ({ page }) => {
  // a project the Projects screen cannot draw: its repositories are not a list
  const broken = { id: "broken", name: "Broken", color: null, emoji: null, image: null, repos: 5 };
  await page.route(/\/api\/v1\/projects$/, (route) => route.fulfill({ json: { projects: [broken], loose: [] } }));
  await page.goto("/#/projects");
  await expect(page.getByRole("heading", { name: "This page could not be shown" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Reload" })).toBeVisible();

  const nav = page.getByRole("navigation", { name: "Main" });
  await nav.getByRole("link", { name: "Inbox" }).click();
  await expect(page.getByRole("heading", { name: "Inbox", level: 1 })).toBeVisible();
  await expect(page.getByRole("heading", { name: "This page could not be shown" })).toHaveCount(0);

  // back on the page that failed, the app tries to draw it again
  await page.unroute(/\/api\/v1\/projects$/);
  await nav.getByRole("link", { name: "Projects" }).click();
  await expect(page.getByRole("heading", { name: "Projects", level: 1 })).toBeVisible();
});
