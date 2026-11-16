// Typing # in an item's comment or description, or in a message to an
// agent, lists the open work items, those of the item's repository first,
// and puts the chosen one's key in the text.

import { expect, test, type APIRequestContext, type Page } from "../fixtures";
import { core } from "../helpers";

let counter = 0;
const fresh = (name: string) => `pick-${Date.now()}-${++counter}/${name}`;

async function add(request: APIRequestContext, repo: string, title: string) {
  const response = await request.post(`${core}/api/v1/items`, {
    data: { repo, title, by: "claude-code", session: "s-pick" },
  });
  expect(response.status(), await response.text()).toBe(201);
  return ((await response.json()) as { key: string }).key;
}

const panel = (page: Page) => page.getByTestId("item-panel");
const picker = (page: Page) => page.getByRole("listbox", { name: "Work items" });

test("# in a comment lists the open items, and Enter puts the chosen key in the text", async ({ page, request }) => {
  const repo = fresh("api");
  const tag = `${Date.now()}`;
  const loader = await add(request, repo, `Quokka loader ${tag}`);
  await add(request, repo, `Rename the wombat table ${tag}`);
  const elsewhere = await add(request, fresh("web"), `Quokka cache ${tag}`);

  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}&open=${loader}`);
  await expect(panel(page).locator(".item-panel-key")).toHaveText(loader);
  const field = panel(page).getByLabel("Add a comment");
  await field.pressSequentially("See #quokka");

  // the item's own repository first, then the others
  const options = picker(page).getByRole("option");
  await expect(options.first()).toContainText(loader);
  await expect(options.nth(1)).toContainText(elsewhere);
  await expect(options.first()).toHaveAttribute("aria-selected", "true");

  await page.keyboard.press("ArrowDown");
  await expect(options.nth(1)).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Enter");
  await expect(picker(page)).toHaveCount(0);
  await expect(field).toHaveValue(`See ${loader} `);

  // Esc closes the list, and leaves the text and the panel
  await page.keyboard.type("and #wombat");
  await expect(picker(page).getByRole("option").first()).toContainText("Rename the wombat table");
  await page.keyboard.press("Escape");
  await expect(picker(page)).toHaveCount(0);
  await expect(panel(page)).toBeVisible();
  await expect(field).toHaveValue(`See ${loader} and #wombat`);
});

test("# in the description lists the items too", async ({ page, request }) => {
  const repo = fresh("docs");
  const tag = `${Date.now()}`;
  const first = await add(request, repo, `Write the guide ${tag}`);
  const other = await add(request, repo, `Draw the zebra diagram ${tag}`);

  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}&open=${first}`);
  await panel(page)
    .locator(".item-section")
    .filter({ has: page.getByRole("heading", { name: /Description/ }) })
    .getByRole("button", { name: "Edit" })
    .click();
  const field = panel(page).getByRole("textbox", { name: "Description" });
  await expect(field).toBeFocused();
  await page.keyboard.type("Needs #zebra");
  await expect(picker(page).getByRole("option").first()).toContainText(other);
  await page.keyboard.press("Tab");
  await expect(field).toHaveValue(`Needs ${other} `);
});

test("# in a message to the agent lists the items, and Enter picks one without sending", async ({ page, request }) => {
  const tag = `${Date.now()}`;
  const key = await add(request, fresh("api"), `Tidy the platypus routes ${tag}`);
  const session = `s-pick-message-${tag}`;
  const status = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { text: "Reading the routes" },
  });
  expect(status.status(), await status.text()).toBe(200);

  await page.goto(`/#/sessions/claude-code/${session}`);
  const box = page.getByRole("textbox", { name: "Message to the agent" });
  await box.pressSequentially("Start on #platypus");
  await expect(picker(page).getByRole("option").first()).toContainText(key);
  await page.keyboard.press("Enter");
  await expect(picker(page)).toHaveCount(0);
  await expect(box).toHaveValue(`Start on ${key} `);
  await expect(page.getByTestId("thread").getByTestId("message")).toHaveCount(0);
});
