import { expect, test } from "@playwright/test";

// macOS's web view bounces the whole page when a scroll goes past an edge,
// so scrolling a sidebar with nothing more to show moved the window's
// contents down and back. The page itself never scrolls; only the panes
// inside it do, and a pane at its end stops there. The bounce needs a
// trackpad and the system's web view, so this checks what it depends on.
test("the page does not bounce when a pane is scrolled past its end", async ({ page }) => {
  await page.goto("/");
  await page.locator(".app-main").waitFor();
  const overscroll = await page.evaluate(() =>
    [document.documentElement, document.body].map((el) => getComputedStyle(el).overscrollBehaviorY),
  );
  expect(overscroll).toEqual(["none", "none"]);
});
