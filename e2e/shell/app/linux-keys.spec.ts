import { expect, test } from "../fixtures";
import { settingsDialog, settingsSection } from "../helpers";

// The app on Linux: keys are named as a Linux keyboard names them
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(navigator, "platform", { get: () => "Linux x86_64" }));
});

test("on Linux, Settings names keys the way a Linux keyboard does", async ({ page }) => {
  await page.goto("/#/");
  await page.keyboard.press("Control+,");
  const settings = settingsDialog(page);
  await expect(settings).toBeVisible();

  await settingsSection(page, "Shortcuts").click();
  await expect(settings).toContainText("Alt");
  await expect(settings).toContainText("Shift");
  await expect(settings).toContainText("Ctrl, Alt or Super");

  await settingsSection(page, "Appearance").click();
  await expect(settings).toContainText("Ctrl+Shift+L");

  for (const section of ["General", "Appearance", "Shortcuts"]) {
    await settingsSection(page, section).click();
    const text = await settings.innerText();
    expect(text, section).not.toMatch(/[⌘⌥⇧⌃]|menu bar/);
  }
});
