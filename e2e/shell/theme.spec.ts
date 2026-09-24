import { expect, test } from "@playwright/test";

test("⌘⇧L switches the theme, and T is left to the plugins", async ({ page }) => {
  await page.goto("/#/history");
  const html = page.locator("html");
  await expect(html).toHaveAttribute("data-theme", /^(light|dark)$/);
  const before = await html.getAttribute("data-theme");
  const other = before === "dark" ? "light" : "dark";

  await page.keyboard.press("t");
  await expect(html).toHaveAttribute("data-theme", before!);

  await page.keyboard.press("ControlOrMeta+Shift+L");
  await expect(html).toHaveAttribute("data-theme", other);
  await page.keyboard.press("ControlOrMeta+Shift+L");
  await expect(html).toHaveAttribute("data-theme", before!);
});
