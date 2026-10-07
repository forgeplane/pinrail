import { expect, test, type Page } from "@playwright/test";
import { clearInbox, createReview } from "./helpers";

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

test("the sidebar toggle stays in place when the sidebar scrolls", async ({ page }) => {
  // the app's smallest window, with more waiting than fits beside it
  await clearInbox(page.request);
  for (let i = 1; i <= 8; i++)
    await createReview(page.request, { title: `Waiting ${i}`, origin: { repo: `acme/repo-${i}`, workflow: "w" } });
  await page.setViewportSize({ width: 1000, height: 480 });
  await page.goto("/#/history");
  await expect(toggle(page)).toHaveAttribute("aria-label", "Hide sidebar");
  const before = (await toggle(page).boundingBox())!;
  const sidebar = page.locator(".sidebar-scroll");
  // outside what scrolls: inside, it would move as the scrolling bounces
  // at either end, which a headless browser does not do
  await expect(sidebar.locator("button.bar-button[aria-label$='sidebar']")).toHaveCount(0);
  expect(await sidebar.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);

  await sidebar.evaluate((el) => el.scrollTo(0, el.scrollHeight));
  expect(await sidebar.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
  expect(await toggle(page).boundingBox()).toEqual(before);
});
