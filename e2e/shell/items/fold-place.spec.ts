// Folding a parent on the Work items page keeps the list where it was, on
// the page and beside an item open in the side panel. It runs in WebKit
// too, whose layout put the list back at its top.

import { expect, test, type APIRequestContext } from "../fixtures";
import { core } from "../helpers";

let n = 0;
const fresh = () => `fold-place/${Date.now()}-${++n}`;

async function add(request: APIRequestContext, repo: string, title: string, extra: Record<string, unknown> = {}) {
  const response = await request.post(`${core}/api/v1/items`, {
    data: { repo, title, by: "claude-code", session: "s-shell", ...extra },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { key: string };
}

for (const beside of [false, true])
  test(`folding or unfolding a parent keeps the list where it was${beside ? ", beside an open item" : ""}`, async ({
    page,
    request,
  }) => {
    const repo = fresh();
    // a list taller than the window, with a parent in its middle
    for (let i = 1; i <= 15; i++) await add(request, repo, `Before ${i}`);
    const parent = await add(request, repo, "Parent in the middle");
    for (let i = 1; i <= 6; i++) await add(request, repo, `Child ${i}`, { parent: parent.key });
    for (let i = 1; i <= 15; i++) await add(request, repo, `After ${i}`);
    await page.setViewportSize({ width: 1400, height: 600 });
    await page.goto(`/#/items?repo=${encodeURIComponent(repo)}`);
    const rows = page.getByTestId("item-row");
    await expect(rows.first()).toBeVisible();
    if (beside) {
      await rows.first().click();
      await expect(page.getByTestId("item-panel")).toBeVisible();
    }
    const at = page.locator(`[data-testid="item-row"][data-key="${parent.key}"]`);
    await at.evaluate((el) => el.scrollIntoView({ block: "center" }));
    const top = async () => (await at.boundingBox())!.y;
    const before = await top();

    await page.getByRole("button", { name: `Hide the sub-items of ${parent.key}`, exact: true }).click();
    await expect(page.getByRole("button", { name: `Show the sub-items of ${parent.key}`, exact: true })).toBeVisible();
    await page.waitForTimeout(200);
    expect(Math.abs((await top()) - before), "folded").toBeLessThan(2);

    await page.getByRole("button", { name: `Show the sub-items of ${parent.key}`, exact: true }).click();
    await expect(page.getByRole("button", { name: `Hide the sub-items of ${parent.key}`, exact: true })).toBeVisible();
    await page.waitForTimeout(200);
    expect(Math.abs((await top()) - before), "unfolded").toBeLessThan(2);
  });
