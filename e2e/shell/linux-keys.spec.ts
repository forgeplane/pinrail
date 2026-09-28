import { expect, test } from "@playwright/test";

// The app on Linux: keys are named as a Linux keyboard names them
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(navigator, "platform", { get: () => "Linux x86_64" }));
});

test("on Linux, Settings names keys the way a Linux keyboard does", async ({ page }) => {
  await page.goto("/#/");
  await page.keyboard.press("Control+,");
  const settings = page.locator("[data-settings]");
  await expect(settings).toBeVisible();

  await page.locator('[data-section="shortcuts"]').click();
  await expect(settings).toContainText("Alt");
  await expect(settings).toContainText("Shift");
  await expect(settings).toContainText("Ctrl, Alt or Super");

  await page.locator('[data-section="appearance"]').click();
  await expect(settings).toContainText("Ctrl+Shift+L");

  for (const section of ["general", "appearance", "shortcuts"]) {
    await page.locator(`[data-section="${section}"]`).click();
    const text = await settings.innerText();
    expect(text, section).not.toMatch(/[⌘⌥⇧⌃]|menu bar/);
  }
});
