// The keyboard-shortcuts dialog: the keys in sections, those of the page
// first, and every section on request or when searching.

import { expect, test, type Page } from "@playwright/test";
import { core } from "./helpers";

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

test("Esc leaves a page's search field, so the page's keys work again", async ({ page, request }) => {
  const repo = `esc-${Date.now()}/app`;
  for (const title of ["First", "Second"]) {
    const response = await request.post(`${core}/api/v1/items`, {
      data: { repo, title, by: "claude-code", session: "s-esc" },
    });
    expect(response.status()).toBe(201);
  }
  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}`);
  const field = page.getByLabel("Search work items");
  await field.fill("Fi");
  await field.press("Escape");
  await expect(field).not.toBeFocused();
  await page.keyboard.press("j");
  await expect(field).toHaveValue("Fi");

  // the Sessions page too
  await page.goto("/#/sessions");
  const sessions = page.getByRole("searchbox", { name: /Search sessions/ });
  await sessions.fill("x");
  await sessions.press("Escape");
  await expect(sessions).not.toBeFocused();
});

test("⌘1 to ⌘4 go to the Inbox, Sessions, Work items and Projects pages", async ({ page }) => {
  await page.goto("/#/projects");
  await expect(page.getByRole("heading", { name: "Projects", level: 1 })).toBeVisible();
  await page.keyboard.press("ControlOrMeta+3");
  await expect(page).toHaveURL(/#\/items$/);
  await page.keyboard.press("ControlOrMeta+2");
  await expect(page).toHaveURL(/#\/sessions$/);
  await page.keyboard.press("ControlOrMeta+4");
  await expect(page).toHaveURL(/#\/projects$/);
  await page.keyboard.press("ControlOrMeta+1");
  await expect(page).toHaveURL(/#\/$/);
  // the sidebar's links say their keys
  await page.getByRole("link", { name: /^Work items/ }).hover();
  await expect(page.getByRole("tooltip")).toContainText("3");
});
