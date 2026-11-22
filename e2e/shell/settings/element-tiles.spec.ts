// The Element tiles switch in Settings › Appearance: off by default, it
// hides the element a session is named after and the periodic table.

import { expect, test } from "../fixtures";
import { core, settingSaved } from "../helpers";

test.afterEach(async ({ request }) => {
  await request.patch(`${core}/api/v1/settings`, { data: { appearance: { element_tiles: false } } });
  await settingSaved(request, (s) => s.appearance.element_tiles, false);
});

test("element tiles show only when the switch in Settings is on", async ({ page, request }) => {
  const session = `s-tiles-${Date.now()}`;
  const started = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { kind: "start", text: "Hide the tiles" },
  });
  expect(started.status(), await started.text()).toBe(200);
  const thread = page.getByTestId("thread");
  const tile = thread.getByTestId("element-tile");
  const palette = page.getByRole("dialog", { name: "Search" });

  // off: the agent's icon in the tile's place, no Element filter, no periodic table
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(thread.getByText("Hide the tiles").first()).toBeVisible();
  const avatar = page.locator(".thread-head > .thread-avatar");
  await expect(avatar.getByRole("img", { name: "Claude Code" })).toBeVisible();
  await expect(tile).toHaveCount(0);
  const box = (await avatar.boundingBox())!;
  expect(Math.round(box.width)).toBe(Math.round(box.height));
  await page.keyboard.press("ControlOrMeta+k");
  await palette.getByRole("textbox").fill("periodic");
  await expect(palette.getByRole("option", { name: /Show the periodic table/ })).toHaveCount(0);
  await page.keyboard.press("Escape");
  await page.goto("/#/sessions");
  const filters = page.getByRole("menu", { name: /^Filter/ });
  await page.getByRole("button", { name: "Filter" }).click();
  await expect(filters).toContainText("Project or repository");
  await expect(filters).not.toContainText("Element");
  await page.keyboard.press("Escape");

  // on: the tile, and the rest, come back
  await page.keyboard.press("ControlOrMeta+,");
  await page.getByRole("button", { name: "Appearance" }).click();
  await page.getByRole("switch", { name: "Element tiles" }).click();
  await settingSaved(request, (s) => s.appearance.element_tiles, true);
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Filter" }).click();
  await expect(filters).toContainText("Element");
  await page.keyboard.press("Escape");
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(tile).toBeVisible();
  await page.keyboard.press("ControlOrMeta+k");
  await palette.getByRole("textbox").fill("periodic");
  await expect(palette.getByRole("option", { name: /Show the periodic table/ })).toBeVisible();
});
