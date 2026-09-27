import { expect, test } from "@playwright/test";
import { core, corePort } from "./helpers";


const served = async (request: import("@playwright/test").APIRequestContext) => (await request.get(`${core}/api/v1/settings`)).json();

test("keep-days is chosen from the menu and read back from the core", async ({ page }) => {
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await expect(page.locator("[data-settings]")).toBeVisible();
  await page.locator('[data-section="data"]').click();

  const menu = page.getByRole("button", { name: "Keep reviews for" });
  await expect(menu).toContainText("Forever");
  await menu.click();
  await page.getByRole("option", { name: "90 days" }).click();
  await expect(menu).toContainText("90 days");
  await expect.poll(async () => (await served(page.request)).history.keep_days).toBe(90);

  await menu.click();
  await page.getByRole("option", { name: "Forever" }).click();
  await expect.poll(async () => (await served(page.request)).history.keep_days).toBeNull();
});

test("the port is stored for the next start and the row says the server has not moved", async ({ page }) => {
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await page.locator('[data-section="data"]').click();

  // the field shows the stored port, which is where the next start listens;
  // the test core was started on another one by hand, so the row says so
  const stored = (await served(page.request)).port;
  const field = page.locator("[data-setting-port]");
  await expect(field).toHaveValue(String(stored));
  await field.fill("4811");
  await field.press("Enter");
  await expect.poll(async () => (await served(page.request)).port).toBe(4811);
  await expect(page.locator(".settings-row", { has: field })).toContainText(`Takes effect when Pinrail starts next; until then the server stays on ${corePort}`);

  // out of range is not sent, and the field goes back to what is stored
  await field.fill("80");
  await field.press("Enter");
  await expect(field).toHaveValue("4811");
  expect((await served(page.request)).port).toBe(4811);

  await field.fill(String(stored));
  await field.press("Enter");
  await expect.poll(async () => (await served(page.request)).port).toBe(stored);
});

test("an older settings answer that arrives last does not undo a newer change", async ({ page }) => {
  // two changes in a row: the answer to the first, held up, must not put
  // back the second's setting when it finally lands
  await page.request.patch(`${core}/api/v1/settings`, { data: { notifications: { enabled: true, sound: true } } });
  let held = false;
  await page.route(`${core}/api/v1/settings`, async (route) => {
    if (route.request().method() === "PATCH" && !held) {
      held = true;
      const response = await route.fetch();
      await new Promise((r) => setTimeout(r, 1000));
      return route.fulfill({ response });
    }
    return route.continue();
  });
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await expect(page.locator("[data-settings]")).toBeVisible();

  const sound = page.getByRole("switch", { name: "Sound" });
  const notifications = page.getByRole("switch", { name: "System notifications" });
  await sound.click();
  await notifications.click();
  await page.waitForTimeout(1500);
  await expect(notifications, "the older answer turned notifications back on").toHaveAttribute("aria-checked", "false");
  expect((await served(page.request)).notifications.enabled).toBe(false);

  await page.request.patch(`${core}/api/v1/settings`, { data: { notifications: { enabled: true, sound: true } } });
});
