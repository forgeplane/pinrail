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

test("a session with a rare element no session had announces its discovery", async ({ page }) => {
  const session = `s-rare-${Date.now()}`;
  await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { kind: "start", text: "Find a rare one" },
  });
  const { session: s } = (await (await page.request.get(`${core}/api/v1/sessions/claude-code/${session}`)).json()) as {
    session: { element: number; name: string };
  };
  // the list says its element is rare, and that no session had it before
  const response = await page.request.get(`${core}/api/v1/elements`);
  const { elements } = (await response.json()) as {
    elements: { number: number; name: string; rarity: string; sessions: number }[];
  };
  const element = elements[s.element - 1];
  element.rarity = "rare";
  element.sessions = 0;
  await page.route(/\/api\/v1\/elements$/, (route) => route.fulfill({ json: { elements } }));

  await page.goto("/#/");
  await expect(page.getByText(`New rare element discovered: ${element.name}, by ${s.name}.`)).toBeVisible();
});

test("the periodic table lights a session's element, describes it, and a click lists its sessions", async ({
  page,
}) => {
  const session = `s-table-${Date.now()}`;
  await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { kind: "start", text: "Fill the table" },
  });
  const { session: s } = (await (await page.request.get(`${core}/api/v1/sessions/claude-code/${session}`)).json()) as {
    session: { element: number };
  };
  const { elements } = (await (await page.request.get(`${core}/api/v1/elements`)).json()) as {
    elements: { name: string; description: string; sessions: number }[];
  };
  const element = elements[s.element - 1];

  await page.goto("/#/sessions");
  await page.getByRole("tab", { name: "Periodic table" }).click();
  await expect(page).toHaveURL(/view=table/);
  const cell = page.getByRole("button", { name: new RegExp(`^${element.name}, \\d+ sessions?$`) });
  await cell.hover();
  const table = page.getByRole("region", { name: "Periodic table of sessions" });
  await expect(table).toContainText(element.description);
  await expect(table.getByRole("link", { name: "Fill the table" })).toBeVisible();
  // an element no session had keeps its secret
  const unfound = elements.findIndex((e) => e.sessions === 0);
  if (unfound >= 0) {
    await page.getByRole("button", { name: `Element ${unfound + 1}, not discovered` }).hover();
    await expect(table).toContainText("Not discovered yet");
    await expect(table).not.toContainText(elements[unfound].description);
  }

  // a click lists the sessions named after it
  await cell.click();
  await expect(page).toHaveURL(new RegExp(`element=${s.element}`));
  await expect(page).not.toHaveURL(/view=table/);
  // (other specs name sessions meanwhile, so it does not count them)
  await expect(page.getByTestId("session-row").filter({ hasText: "Fill the table" })).toHaveCount(1);
});
