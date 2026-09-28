import { expect, test } from "@playwright/test";
import { clearInbox, createReview } from "./helpers";

test("the inbox is grouped by project or one list, newest first, and remembers which", async ({ page }) => {
  await clearInbox(page.request);
  // oldest first: zeta gets the oldest and the newest, acme the one between
  await createReview(page.request, { title: "Layout one", origin: { repo: "zeta/app", workflow: "layout" } });
  await createReview(page.request, { title: "Layout two", origin: { repo: "acme/api", workflow: "layout" } });
  await createReview(page.request, { title: "Layout three", origin: { repo: "zeta/app", workflow: "layout" } });

  await page.goto("/#/");
  const rows = page.locator("[data-review-row]");
  // grouped: the project with the newest review first, then newest within it
  await expect(page.locator(".inbox-repo summary strong")).toHaveText(["zeta/app", "acme/api"]);
  await expect(rows).toHaveText([/Layout three/, /Layout one/, /Layout two/]);

  await page.getByRole("radio", { name: "One list" }).click();
  await expect(page.locator(".inbox-repo")).toHaveCount(0);
  await expect(rows).toHaveText([/Layout three/, /Layout two/, /Layout one/]);
  // with no project headings, each row says its project
  await expect(rows.nth(1).locator(".review-row-project")).toHaveText("acme/api");

  // J walks the list as it is drawn
  await page.keyboard.press("j");
  await expect(rows.nth(1)).toHaveClass(/is-focused/);

  // the choice outlasts a reload
  await page.reload();
  await expect(page.getByRole("radio", { name: "One list" })).toHaveAttribute("aria-checked", "true");
  await expect(rows).toHaveText([/Layout three/, /Layout two/, /Layout one/]);

  await page.getByRole("radio", { name: "Grouped by project" }).click();
  await expect(page.locator(".inbox-repo")).toHaveCount(2);
});

test("the inbox's keys stay quiet under a dialog, and with a modifier", async ({ page }) => {
  // d discards the focused review: not from behind Settings, and not as ⌘D
  await clearInbox(page.request);
  await createReview(page.request, { title: "Quiet keys" });
  const discard = page.locator(".discard-dialog");

  await page.goto("/#/");
  await expect(page.locator("[data-review-row]")).toHaveCount(1);
  await page.keyboard.press("ControlOrMeta+,");
  await expect(page.locator("[data-settings]")).toBeVisible();
  await page.keyboard.press("d");
  await expect(discard, "d reached the inbox behind Settings").toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(page.locator("[data-settings]")).toHaveCount(0);

  await page.keyboard.press("ControlOrMeta+d");
  await expect(discard, "⌘D opened the discard dialog").toHaveCount(0);
  // d alone still does
  await page.keyboard.press("d");
  await expect(discard).toBeVisible();
});
