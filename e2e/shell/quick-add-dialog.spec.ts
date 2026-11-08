// Adding a work item from any page with ⌘⇧C: a dialog with the Work items
// page's quick add, which on that page opens in place.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core } from "./helpers";

let counter = 0;
const fresh = (name = "repo") => `qd-${Date.now()}-${++counter}/${name}`;

async function add(request: APIRequestContext, repo: string, title: string) {
  const response = await request.post(`${core}/api/v1/items`, {
    data: { repo, title, by: "claude-code", session: "s-quick" },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { key: string };
}

async function listed(request: APIRequestContext, repo: string) {
  const response = await request.get(`${core}/api/v1/items?repo=${encodeURIComponent(repo)}`);
  return ((await response.json()) as { items: { key: string; title: string; repo: string }[] }).items.filter(
    (i) => i.repo === repo,
  );
}

const dialog = (page: Page) => page.getByRole("dialog", { name: "Add a work item" });
const field = (page: Page) => dialog(page).getByRole("textbox", { name: "What needs to be done?" });

test("⌘⇧C adds a work item from any page, which stays where it is", async ({ page, request }) => {
  const repo = fresh("app");
  await add(request, repo, "Existing");
  // the inbox, narrowed to the repository the item goes in
  await page.goto(`/#/?repo=${encodeURIComponent(repo)}`);
  await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();

  await page.keyboard.press("ControlOrMeta+Shift+C");
  await expect(field(page)).toBeFocused();
  await expect(dialog(page).getByRole("button", { name: "Repository" })).toHaveText(repo);

  // ⌘Enter adds it and keeps the dialog for the next one
  await field(page).fill("First from anywhere");
  await field(page).press("ControlOrMeta+Enter");
  await expect(field(page)).toHaveValue("");
  await expect(dialog(page)).toBeVisible();
  await expect.poll(async () => (await listed(request, repo)).map((i) => i.title)).toContain("First from anywhere");

  // Enter adds it and closes the dialog, leaving the page as it was, and
  // a toast names the item
  await field(page).fill("Second from anywhere");
  await field(page).press("Enter");
  await expect(dialog(page)).toHaveCount(0);
  const second = (await listed(request, repo)).find((i) => i.title === "Second from anywhere")!;
  await expect(page.getByText(`${second.key} added`)).toBeVisible();
  await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();

  // Esc closes it, adding nothing
  await page.goto("/#/sessions");
  await page.keyboard.press("ControlOrMeta+Shift+C");
  await field(page).fill("Never added");
  await field(page).press("Escape");
  await expect(dialog(page)).toHaveCount(0);
  expect((await listed(request, repo)).map((i) => i.title)).not.toContain("Never added");
});

test("on the Work items page, ⌘⇧C opens the page's own quick add", async ({ page, request }) => {
  const repo = fresh("app");
  await add(request, repo, "Existing");
  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}`);
  await expect(page.getByRole("heading", { name: "Work items", level: 1 })).toBeVisible();
  await page.keyboard.press("ControlOrMeta+Shift+C");
  await expect(
    page.getByTestId("item-quick-box").getByRole("textbox", { name: "What needs to be done?" }),
  ).toBeFocused();
  await expect(dialog(page)).toHaveCount(0);
});

test("the command palette offers to add a work item", async ({ page }) => {
  await page.goto("/#/");
  await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await palette.getByRole("textbox").fill("Add a work item");
  // the action, not a work item whose title says so
  await palette
    .getByRole("option")
    .filter({ has: page.locator(".palette-title", { hasText: /^Add a work item$/ }) })
    .click();
  await expect(field(page)).toBeFocused();
});

test("from a session or a review, the dialog starts on its repository", async ({ page, request }) => {
  const repo = fresh("session-repo");
  const session = `s-qd-${Date.now()}`;
  const started = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { text: "Load the themes", kind: "start", origin: { repo } },
  });
  expect(started.status(), await started.text()).toBe(200);
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(page.getByText("Load the themes").first()).toBeVisible();
  await page.keyboard.press("ControlOrMeta+Shift+C");
  await expect(field(page)).toBeFocused();
  await expect(dialog(page).getByRole("button", { name: "Repository" })).toHaveText(repo);
  await field(page).press("Escape");

  const other = fresh("review-repo");
  const sent = await request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "list",
      title: "Theme loader",
      origin: { repo: other },
      requested_by: "claude-code",
      payload: { groups: [{ title: "g", items: [{ id: 1, title: "x" }] }] },
    },
  });
  expect(sent.status(), await sent.text()).toBe(201);
  const id = ((await sent.json()) as { id: string }).id;
  await page.goto(`/#/reviews/${id}`);
  await expect(page.getByTestId("review-strip")).toBeVisible();
  await page.keyboard.press("ControlOrMeta+Shift+C");
  await expect(dialog(page).getByRole("button", { name: "Repository" })).toHaveText(other);
  await field(page).press("Escape");
  await request.post(`${core}/api/v1/reviews/${id}/discard`, { data: { reason: "spec cleanup" } });
});
