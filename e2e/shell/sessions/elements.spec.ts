// A session's element, in its header's avatar place, with its card, while
// Settings shows the element tiles.

import { expect, test } from "../fixtures";
import { core, sessionId, settingSaved } from "../helpers";

// the element tiles show only while Settings › Appearance turns them on
test.beforeEach(async ({ request }) => {
  await request.patch(`${core}/api/v1/settings`, { data: { appearance: { element_tiles: true } } });
  await settingSaved(request, (s) => s.appearance.element_tiles, true);
});

// a test may end while the page waits for an answer a route gives it
test.afterEach(async ({ page, request }) => {
  await page.unrouteAll({ behavior: "ignoreErrors" });
  await request.patch(`${core}/api/v1/settings`, { data: { appearance: { element_tiles: false } } });
});

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
  const id = await sessionId(page.request, "claude-code", session);
  const row = page.locator(`[data-testid="sidebar-session"][href="#/sessions/claude-code/${id}"]`);
  await expect(row).toBeVisible();
  // the thread's own load does not answer
  await page.route(new RegExp(`/api/v1/sessions/claude-code/${id}$`), () => {});
  await row.click();
  await expect(page.getByTestId("thread").getByRole("heading", { level: 1 })).toHaveText("Show the header at once");
  await expect(page.locator(".thread-head > .element-tile")).toBeVisible();
});

test("a session with a rare element no session had announces its discovery", async ({ page }) => {
  const session = `s-rare-${Date.now()}`;
  const status = `${core}/api/v1/sessions/claude-code/${session}/status`;
  await page.request.post(status, { data: { kind: "start", text: "Find a rare one" } });
  const { session: s } = (await (await page.request.get(`${core}/api/v1/sessions/claude-code/${session}`)).json()) as {
    session: { id: string; element: number; generated_name: string };
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
  // and so do the lists of sessions: none has the element, until this
  // session starts while the app is open
  let started = false;
  await page.route(/\/api\/v1\/sessions\?/, async (route) => {
    const answer = await route.fetch();
    const body = (await answer.json()) as { sessions: { id: string; element: number }[] };
    body.sessions = body.sessions.filter((other) => other.element !== s.element || (started && other.id === s.id));
    await route.fulfill({ response: answer, json: body });
  });

  // its thread shows the element once the app has the list of elements
  await page.goto(`/#/sessions/claude-code/${s.id}`);
  await expect(
    page.getByTestId("thread").getByRole("img", { name: `${element.name}, element ${s.element}` }),
  ).toBeVisible();
  started = true;
  await page.request.post(status, { data: { text: "Looking" } });
  await expect(page.getByText(`New rare element discovered: ${element.name}, by ${s.generated_name}.`)).toBeVisible();
});

test("a list of sessions asked for before the settings loaded announces a discovery in it", async ({ page }) => {
  const session = `s-rare-early-${Date.now()}`;
  await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { kind: "start", text: "Find a rare one early" },
  });
  const { session: s } = (await (await page.request.get(`${core}/api/v1/sessions/claude-code/${session}`)).json()) as {
    session: { id: string; element: number; generated_name: string };
  };
  const { elements } = (await (await page.request.get(`${core}/api/v1/elements`)).json()) as {
    elements: { number: number; name: string; rarity: string; sessions: number }[];
  };
  const element = elements[s.element - 1];
  element.rarity = "rare";
  element.sessions = 0;
  await page.route(/\/api\/v1\/elements$/, (route) => route.fulfill({ json: { elements } }));
  // no list of sessions answers until the app has the settings and the
  // elements, the first of them asked for before either
  let ready = () => {};
  const loaded = new Promise<void>((resolve) => (ready = resolve));
  await page.route(/\/api\/v1\/sessions\?/, async (route) => {
    const answer = await route.fetch();
    const body = (await answer.json()) as { sessions: { id: string; element: number }[] };
    body.sessions = body.sessions.filter((other) => other.element !== s.element || other.id === s.id);
    await loaded;
    await route.fulfill({ response: answer, json: body });
  });

  await page.goto(`/#/sessions/claude-code/${s.id}`);
  await expect(
    page.getByTestId("thread").getByRole("img", { name: `${element.name}, element ${s.element}` }),
  ).toBeVisible();
  ready();
  await expect(page.getByText(`New rare element discovered: ${element.name}, by ${s.generated_name}.`)).toBeVisible();
});

test("the periodic table lights a session's element, describes it, and a click keeps its card", async ({ page }) => {
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

  await page.goto("/#/elements");
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

  // a click keeps the element's card after the pointer leaves the table,
  // so that its sessions can be opened from the card
  await cell.click();
  await expect(page).toHaveURL(/#\/elements$/);
  await page.getByText(/elements discovered/).hover();
  await expect(table).toContainText(element.description);
  await table.getByRole("link", { name: "Fill the table" }).click();
  await expect(page).toHaveURL(
    new RegExp(`/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}`),
  );
});

test("the periodic table opens from the command palette over the whole window, and returns to the page before", async ({
  page,
}) => {
  await page.request.post(`${core}/api/v1/sessions/claude-code/s-palette-${Date.now()}/status`, {
    data: { kind: "start", text: "Open the table" },
  });
  await page.goto("/#/sessions");
  await expect(page.getByTestId("session-row").first()).toBeVisible();
  await expect(page.getByRole("link", { name: "Periodic table" })).toHaveCount(0);
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await palette.getByRole("textbox").fill("periodic");
  await palette.getByRole("option", { name: /Show the periodic table/ }).click();
  await expect(page).toHaveURL(/#\/elements$/);
  await expect(page.getByRole("region", { name: "Periodic table of sessions" })).toBeVisible();
  // it takes the whole window
  await expect(page.getByRole("link", { name: "Inbox" })).toHaveCount(0);
  await page.getByRole("button", { name: "Back to the app" }).click();
  await expect(page).toHaveURL(/#\/sessions$/);
  await expect(page.getByRole("link", { name: "Inbox" }).first()).toBeVisible();
});
