import { expect, test } from "./fixtures";
import { core, settingsDialog, settingsSection } from "./helpers";

test.afterEach(({ page }) => page.unrouteAll({ behavior: "ignoreErrors" }));

const runtime = async (request: import("@playwright/test").APIRequestContext) =>
  (await request.get(`${core}/api/v1/runtime`)).json();

test("the Labs switch turns the agent runtime on and off, and follows the settings file", async ({ page }) => {
  await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: false } } });
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await expect(settingsDialog(page)).toBeVisible();
  await settingsSection(page, "Labs").click();

  const toggle = page.getByRole("switch", { name: "Run agents in Pinrail" });
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await expect(settingsDialog(page)).toContainText("keeps them running after the app quits");

  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");
  await expect.poll(async () => (await runtime(page.request)).enabled).toBe(true);

  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await expect.poll(async () => (await runtime(page.request)).enabled).toBe(false);

  // a change made elsewhere, as an edit of settings.json is, shows at once
  await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: true } } });
  await expect(toggle).toHaveAttribute("aria-checked", "true");
  await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: false } } });
  await expect(toggle).toHaveAttribute("aria-checked", "false");
});
