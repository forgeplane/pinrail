import { expect, test, type Locator } from "@playwright/test";

// How many lines an inline element takes: one box per line it is laid on.
const lines = (code: Locator) => code.evaluate((el) => el.getClientRects().length);

test.describe("code in a docs table, at a phone's width", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("a name stays on one line", async ({ page }) => {
    await page.goto("/docs/plugins/more/");
    const name = page.locator("td code", { hasText: /^visual-diff$/ });
    await expect(name).toBeVisible();
    expect(await lines(name)).toBe(1);
  });

  test("a command still wraps at its spaces", async ({ page }) => {
    await page.goto("/docs/agents/cli/");
    const command = page.locator("td code", { hasText: /^pinrail plugins new <name>/ });
    await expect(command).toBeVisible();
    expect(await lines(command)).toBeGreaterThan(1);
  });
});
