// A session's element, in its header's avatar place, with its card.

import { expect, test } from "../fixtures";
import { core } from "../helpers";

test("a session's header shows its element with the agent on its corner, and its card on hover", async ({ page }) => {
  const session = `s-element-${Date.now()}`;
  const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { kind: "start", text: "Fix the login redirect" },
  });
  expect(response.status(), await response.text()).toBe(200);
  const { session: s } = (await (await page.request.get(`${core}/api/v1/sessions/claude-code/${session}`)).json()) as {
    session: { element: number };
  };
  const { elements } = (await (await page.request.get(`${core}/api/v1/elements`)).json()) as {
    elements: { name: string; description: string; rarity: string }[];
  };
  const element = elements[s.element - 1];

  await page.goto(`/#/sessions/claude-code/${session}`);
  const tile = page.getByTestId("thread").getByRole("img", { name: `${element.name}, element ${s.element}` });
  await expect(tile).toBeVisible();
  await expect(tile.getByRole("img", { name: "Claude Code" })).toBeVisible();

  await tile.hover();
  const card = page.getByRole("tooltip");
  await expect(card).toContainText(element.name);
  await expect(card).toContainText(element.description);
  await expect(card).toContainText(element.rarity, { ignoreCase: true });
});

test("the element is a square that spans the header's two rows exactly", async ({ page }) => {
  const session = `s-align-${Date.now()}`;
  await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { kind: "start", text: "Line up the header", origin: { repo: "acme/api", ref: "main" } },
  });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const box = async (selector: string) => (await page.locator(selector).first().boundingBox())!;
  await expect(page.locator(".thread-head > .element-tile")).toBeVisible();
  const [tile, title, meta] = [
    await box(".thread-head > .element-tile"),
    await box(".thread-title-row"),
    await box(".thread-meta"),
  ];
  expect(Math.abs(tile.y - title.y)).toBeLessThan(1);
  expect(Math.abs(tile.y + tile.height - (meta.y + meta.height))).toBeLessThan(1);
  // and is a square
  expect(Math.abs(tile.width - tile.height)).toBeLessThan(1);
});

test("a session opened from the sidebar shows its element before its thread has loaded", async ({ page }) => {
  const session = `s-early-${Date.now()}`;
  await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { kind: "start", text: "Show the header at once", origin: { repo: "acme/api" } },
  });
  await page.goto("/#/");
  const row = page.locator(`[data-testid="sidebar-session"][href="#/sessions/claude-code/${session}"]`);
  await expect(row).toBeVisible();
  // the thread's own load does not answer
  await page.route(new RegExp(`/api/v1/sessions/claude-code/${session}$`), () => {});
  await row.click();
  await expect(page.getByTestId("thread").getByRole("heading", { level: 1 })).toHaveText("Show the header at once");
  await expect(page.locator(".thread-head > .element-tile")).toBeVisible();
});
