// The keyboard-shortcuts dialog: the keys in sections, those of the page
// first, and every section on request or when searching.

import { expect, test, type Page } from "@playwright/test";

const dialog = (page: Page) => page.getByRole("dialog", { name: "Keyboard shortcuts" });
const sections = (page: Page) => dialog(page).getByRole("heading", { level: 3 });

test("? shows the keys of the page in sections, and every section on request", async ({ page }) => {
  await page.goto("/#/items");
  await expect(page.getByRole("heading", { name: "Work items", level: 1 })).toBeVisible();
  await page.keyboard.press("?");
  await expect(sections(page)).toHaveText(["Work items", "General"]);
  await expect(dialog(page)).toContainText("Queue the work item");
  await expect(dialog(page)).not.toContainText("Archive or unarchive the session");

  await dialog(page).getByRole("button", { name: "Show all shortcuts" }).click();
  await expect(sections(page)).toHaveText(["Work items", "General", "Inbox", "Review", "Sessions"]);
  await page.keyboard.press("Escape");

  // another page, other keys
  await page.goto("/#/sessions");
  await page.keyboard.press("?");
  await expect(sections(page)).toHaveText(["Sessions", "General"]);
  await expect(dialog(page)).toContainText("Archive or unarchive the session");
  await expect(dialog(page)).toContainText("Next / previous session");
});

test("a search looks through every section", async ({ page }) => {
  await page.goto("/#/projects");
  await page.keyboard.press("?");
  await expect(sections(page)).toHaveText(["General"]);
  await dialog(page).getByRole("searchbox", { name: "Search shortcuts" }).fill("queue");
  await expect(sections(page)).toHaveText(["Work items"]);
  await expect(dialog(page)).toContainText("Queue the work item");
});
