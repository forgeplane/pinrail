import { expect, test, type Page } from "@playwright/test";

/** The macOS title bar, where the window's buttons overlay the top left and
 *  end where the app measured them: further right on macOS 26 than on 15. */
async function windowButtonsEndAt(page: Page, end: number) {
  await page.evaluate((end) => {
    document.querySelector(".app-frame")!.classList.add("has-overlay-bar");
    document.documentElement.style.setProperty("--window-buttons-end", `${end}px`);
  }, end);
}

const toggle = (page: Page) => page.locator("button.bar-button[aria-label$='sidebar']");

for (const end of [66, 76]) {
  test(`the sidebar toggle keeps clear of window buttons that end at ${end}px`, async ({ page }) => {
    await page.goto("/#/history");
    await expect(toggle(page)).toHaveAttribute("aria-label", "Hide sidebar");
    await windowButtonsEndAt(page, end);
    await expect.poll(async () => (await toggle(page).boundingBox())!.x).toBe(end + 4);

    await page.keyboard.press("ControlOrMeta+b");
    await expect(toggle(page)).toHaveAttribute("aria-label", "Show sidebar");
    await windowButtonsEndAt(page, end);
    await expect.poll(async () => (await toggle(page).boundingBox())!.x).toBe(end + 4);

    await page.keyboard.press("ControlOrMeta+b");
    await expect(toggle(page)).toHaveAttribute("aria-label", "Hide sidebar");
  });
}
