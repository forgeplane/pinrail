// The Sessions page with sessions opened beside the list: the session
// clicked stays where it was on the screen as the panel opens and closes.
// Sessions open beside the list by default (/inbox/open_in).

import { expect, test } from "../fixtures";
import { core } from "../helpers";

test("a session clicked low in a long list stays where it was as its panel opens and closes", async ({
  page,
  request,
}) => {
  const word = `keepinview${Date.now()}`;
  for (let i = 1; i <= 40; i++) {
    const response = await request.post(`${core}/api/v1/sessions/claude-code/${word}-${i}/status`, {
      data: { kind: "start", text: `Task ${word} ${i}` },
    });
    expect(response.status(), await response.text()).toBeLessThan(300);
  }
  await page.goto(`/#/sessions?q=${word}`);
  const rows = page.getByTestId("session-row");
  await expect(rows).toHaveCount(40);

  const target = rows.nth(30);
  await target.scrollIntoViewIfNeeded();
  const before = (await target.boundingBox())!;
  expect(before.y).toBeGreaterThan(200);

  await target.click();
  const panel = page.getByTestId("session-panel");
  await expect(panel).toBeVisible();
  await expect(target).toBeInViewport();
  expect(Math.abs((await target.boundingBox())!.y - before.y)).toBeLessThan(40);

  await page.keyboard.press("Escape");
  await expect(panel).toBeHidden();
  await expect(target).toBeInViewport();
  expect(Math.abs((await target.boundingBox())!.y - before.y)).toBeLessThan(40);
});
