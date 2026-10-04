import { expect, test } from "@playwright/test";

// The browser build has no native side: the section is there, and says the
// desktop app does the connecting. The skill itself is tested in the app's
// own crate.
test("Agents lists the skills, and the command line stays in Data", async ({ page }) => {
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await page.locator('[data-section="agents"]').click();
  const body = page.locator(".settings-body");
  await expect(body.getByRole("heading", { name: "Agents", level: 2 })).toBeVisible();
  await expect(body.getByRole("heading", { name: "Command line" })).toHaveCount(0);
  await expect(body.getByRole("heading", { name: "Connect your agents" })).toBeVisible();
  await expect(body.getByText("The desktop app finds your agents and connects them")).toBeVisible();

  await page.locator('[data-section="data"]').click();
  await expect(body.getByRole("heading", { name: "Command line" })).toBeVisible();
  await expect(body.getByText("Install the CLI")).toBeVisible();
});
