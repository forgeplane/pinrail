// The Work items page: rows, grouping, nested and flat, the filters, the
// quick add, the keys and live updates. Every test works in repositories of
// its own and opens the page filtered to them, since the shard's core is
// shared.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core, settingSaved, shellOrigin } from "./helpers";

let counter = 0;
/** A repository no other test uses. */
const fresh = (name = "repo") => `wi-${Date.now()}-${++counter}/${name}`;

/** Adds an item as an agent would, and returns it. */
async function add(
  request: APIRequestContext,
  repo: string,
  title: string,
  extra: Record<string, unknown> = {},
  by = "claude-code",
) {
  const response = await request.post(`${core}/api/v1/items`, {
    data: { repo, title, by, session: "s-shell", ...extra },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { key: string; id: string };
}

/** Changes an item as an agent would. */
async function edit(request: APIRequestContext, key: string, change: Record<string, unknown>, session = "s-shell") {
  const response = await request.patch(`${core}/api/v1/items/${key}`, {
    data: { by: "claude-code", session, ...change },
  });
  expect(response.status(), await response.text()).toBe(200);
}

const rows = (page: Page) => page.getByTestId("item-row");
const row = (page: Page, key: string) => page.locator(`[data-testid="item-row"][data-key="${key}"]`);
const keys = (page: Page) => rows(page).evaluateAll((els) => els.map((e) => e.getAttribute("data-key")));

/** The page, filtered to one repository, or to a project with `project:`. */
async function open(page: Page, scope: string, extra = "") {
  const param = scope.startsWith("project:") ? `project=${scope.slice(8)}` : `repo=${encodeURIComponent(scope)}`;
  await page.goto(`/#/items?${param}${extra}`);
  await expect(page.getByRole("heading", { name: "Work items", level: 1 })).toBeVisible();
}

// the layout and the nesting are settings the core keeps: each test leaves
// the defaults for the next
test.afterEach(async ({ request }) => {
  await request.patch(`${core}/api/v1/settings`, { data: { items: { layout: "project", nesting: "nested" } } });
});

test("a row shows the status, key, title, what waits, what blocks it, its progress and who holds it", async ({
  page,
  request,
}) => {
  const repo = fresh("rows");
  const parent = await add(request, repo, "Custom themes");
  const part = await add(request, repo, "Theme file format", { parent: parent.key });
  const held = await add(request, repo, "Load themes", { parent: parent.key, depends_on: [part.key] });
  await edit(request, held.key, { claim: "claim" });
  await add(request, repo, "Theme picker");
  const reviewed = await add(request, repo, "Reviewed");
  const sent = await request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "list",
      title: "Loader",
      origin: { repo },
      requested_by: "claude-code",
      item: reviewed.key,
      payload: { groups: [{ title: "g", items: [{ id: 1, title: "x" }] }] },
    },
  });
  expect(sent.status()).toBe(201);
  await open(page, repo);
  await page.screenshot({ path: test.info().outputPath("rows.png") });

  const heldRow = row(page, held.key);
  await expect(heldRow).toContainText(held.key);
  await expect(heldRow).toContainText("Load themes");
  await expect(heldRow.getByTestId("item-blocked")).toHaveText(`blocked by ${part.key}`);
  await expect(heldRow.getByRole("img", { name: "In progress" })).toBeVisible();
  await expect(heldRow.locator(".item-claim .agent-icon")).toBeVisible();
  await expect(row(page, parent.key).locator(".item-progress")).toHaveText("0/2");
  await expect(row(page, reviewed.key).getByTestId("item-waiting")).toHaveText("waiting on you");
  await expect(row(page, part.key).getByRole("img", { name: "Open" })).toBeVisible();

  // a closed item is shown dimmed, with every status
  await edit(request, part.key, { status: "done" });
  await open(page, repo, "&status=all");
  await expect(row(page, part.key)).toHaveClass(/is-closed/);
  await expect(row(page, part.key).getByRole("img", { name: "Done" })).toBeVisible();
  await expect(row(page, parent.key).locator(".item-progress")).toHaveText("1/2");
  // the inbox is shared with the other tests
  await request.post(`${core}/api/v1/reviews/${((await sent.json()) as { id: string }).id}/discard`, {
    data: { reason: "spec cleanup" },
  });
});

test("sub-items are nested under their parent and fold with it, or flat, naming their parent", async ({
  page,
  request,
}) => {
  const repo = fresh("tree");
  const parent = await add(request, repo, "Release");
  const child = await add(request, repo, "Notes", { parent: parent.key });
  const grandchild = await add(request, repo, "Proofread", { parent: child.key });
  await open(page, repo);
  expect(await keys(page)).toEqual([parent.key, child.key, grandchild.key]);
  // each level is indented further
  const indent = async (key: string) => row(page, key).evaluate((e) => parseFloat(getComputedStyle(e).paddingLeft));
  expect(await indent(child.key)).toBeGreaterThan(await indent(parent.key));
  expect(await indent(grandchild.key)).toBeGreaterThan(await indent(child.key));

  await page.getByRole("button", { name: `Hide the sub-items of ${parent.key}`, exact: true }).click();
  expect(await keys(page)).toEqual([parent.key]);
  await page.getByRole("button", { name: `Show the sub-items of ${parent.key}`, exact: true }).click();
  expect(await keys(page)).toEqual([parent.key, child.key, grandchild.key]);

  await page.getByRole("radio", { name: "Flat" }).click();
  await settingSaved(request, (s) => s.items.nesting, "flat");
  await expect(row(page, child.key).locator(".item-parent")).toHaveText(`in ${parent.key}`);
  expect(await indent(child.key)).toBe(await indent(parent.key));
  // the choice is kept
  await page.reload();
  await expect(page.getByRole("radio", { name: "Flat" })).toHaveAttribute("aria-checked", "true");
  await expect(row(page, grandchild.key).locator(".item-parent")).toHaveText(`in ${child.key}`);
});

test("nested, a sub-item whose parent the filters hide names its parent", async ({ page, request }) => {
  const repo = fresh("orphan");
  const parent = await add(request, repo, "Parent");
  const child = await add(request, repo, "Child", { parent: parent.key });
  await edit(request, parent.key, { claim: "claim" });
  // open items only: the parent, in progress, is left out
  await open(page, repo, "&status=open");
  expect(await keys(page)).toEqual([child.key]);
  await expect(row(page, child.key).locator(".item-parent")).toHaveText(`in ${parent.key}`);
});

test("the items are grouped by project, by repository or in one list, and the choice is kept", async ({
  page,
  request,
}) => {
  const api = fresh("api");
  const web = api.replace("/api", "/web");
  const created = await request.post(`${core}/api/v1/projects`, {
    data: { name: `Grouped ${Date.now()}`, repos: [api, web] },
  });
  const project = (await created.json()) as { id: string; name: string };
  const a = await add(request, api, "In the API");
  const w = await add(request, web, "In the site");
  await open(page, `project:${project.id}`);
  const headings = page.getByTestId("inbox-group-heading");
  await expect(headings).toHaveCount(1);
  await expect(headings).toContainText(project.name);
  // in a project of several repositories, each row names its repository
  await expect(row(page, a.key).locator(".item-repo")).toHaveText(api);

  await page.getByRole("radio", { name: "Grouped by repository" }).click();
  await settingSaved(request, (s) => s.items.layout, "repository");
  await expect(headings).toHaveCount(2);
  await expect(row(page, a.key).locator(".item-repo")).toHaveCount(0);

  await page.getByRole("radio", { name: "One list" }).click();
  await settingSaved(request, (s) => s.items.layout, "list");
  await expect(headings).toHaveCount(0);
  await expect(row(page, w.key).locator(".item-repo")).toHaveText(web);
  await page.reload();
  await expect(page.getByRole("radio", { name: "One list" })).toHaveAttribute("aria-checked", "true");
  await request.delete(`${core}/api/v1/projects/${project.id}`);
});

test("the filters narrow the items by status, author, what waits and words, and clear together", async ({
  page,
  request,
}) => {
  const repo = fresh("filters");
  const ready = await add(request, repo, "Ready theme work");
  const blocked = await add(request, repo, "Blocked", { depends_on: [ready.key] });
  const done = await add(request, repo, "Done already");
  await edit(request, done.key, { status: "done" });
  const mine = await add(request, repo, "By another agent", {}, "codex");
  // the person adds from the app's window
  const person = await request.post(`${core}/api/v1/items`, {
    data: { repo, title: "Mine, from the app" },
    headers: { origin: shellOrigin },
  });
  const personKey = ((await person.json()) as { key: string }).key;
  await open(page, repo);
  expect(await keys(page)).toEqual([ready.key, blocked.key, mine.key, personKey]);

  // by default the list shows the items open or in progress, which it calls open
  await expect(page.getByRole("button", { name: "Status", exact: true })).toHaveText("Open");
  const choose = async (label: string, option: string) => {
    await page.getByRole("button", { name: label, exact: true }).click();
    await page.getByRole("option", { name: option, exact: true }).click();
  };
  await choose("Status", "Ready");
  await expect.poll(() => keys(page)).toEqual([ready.key, mine.key, personKey]);
  await choose("Status", "Blocked");
  await expect.poll(() => keys(page)).toEqual([blocked.key]);
  await choose("Status", "Done");
  await expect.poll(() => keys(page)).toEqual([done.key]);
  await choose("Status", "Open");
  await choose("Created by", "Created by you");
  await expect.poll(() => keys(page)).toEqual([personKey]);
  await choose("Created by", "Created by anyone");
  await page.getByRole("button", { name: "Waiting on you" }).click();
  await expect(page.getByText("No matching work items")).toBeVisible();
  const clear = page.getByRole("button", { name: "Clear filters" });
  await expect(clear.locator("svg")).toBeVisible();
  await clear.click();
  // clearing drops the repository too; the list holds others' items now
  await expect(page).not.toHaveURL(/repo=/);
  await open(page, repo);
  // `/` goes to the search, and every word must match
  await page.locator("body").click({ position: { x: 600, y: 5 } });
  await page.keyboard.press("/");
  await expect(page.getByLabel("Search work items")).toBeFocused();
  await page.keyboard.type("theme ready");
  await expect.poll(() => keys(page)).toEqual([ready.key]);
});

test("the quick add opens with C, adds with Enter, keeps going with ⌘Enter, and cancels with Esc", async ({
  page,
  request,
}) => {
  const repo = fresh("quick");
  const other = repo.replace("/quick", "/other");
  const parent = await add(request, repo, "Parent");
  await add(request, other, "Elsewhere");
  await open(page, repo);
  await row(page, parent.key).hover();
  await page.keyboard.press("c");
  const box = page.getByTestId("item-quick-box");
  const field = box.getByLabel("What needs to be done?");
  await expect(field).toBeFocused();
  // the field is drawn as a field, with the accent's border while focused
  const border = () =>
    field.evaluate((e) => {
      const style = getComputedStyle(e);
      return { width: style.borderTopWidth, color: style.borderTopColor };
    });
  const accent = await page.evaluate(() => {
    const probe = document.createElement("span");
    probe.style.color = "var(--accent)";
    document.body.append(probe);
    const color = getComputedStyle(probe).color;
    probe.remove();
    return color;
  });
  await expect.poll(border).toEqual({ width: "1px", color: accent });
  // the keys it takes, each key or combination in a cap of its own
  const hints = box.locator(".key-hint");
  await expect(hints).toHaveText([
    "EnterAdd and open",
    /\+EnterAdd and add more$/,
    /\+TabChange repository$/,
    "EscCancel",
  ]);
  await expect(hints.locator(".key-cap")).toHaveCount(4);
  // it starts in the repository the list is filtered to
  await expect(box.getByRole("button", { name: "Repository", exact: true })).toContainText(repo);
  // an empty title adds nothing
  await page.keyboard.press("Enter");
  await expect(box).toBeVisible();
  await field.fill("Write the notes");
  await page.keyboard.press("Enter");
  await expect(box).toHaveCount(0);
  const added = rows(page).filter({ hasText: "Write the notes" });
  await expect(added).toHaveClass(/is-open/);
  await expect(page).toHaveURL(/open=/);

  // ⌘Enter keeps the field open for the next one
  await page.getByTestId("item-quick-add").click();
  await field.fill("First of two");
  await page.keyboard.press("ControlOrMeta+Enter");
  await expect(field).toHaveValue("");
  await expect(field).toBeFocused();
  await field.fill("Second of two");
  await page.keyboard.press("ControlOrMeta+Enter");
  await expect(rows(page).filter({ hasText: "of two" })).toHaveCount(2);

  // ⇧Tab changes the repository, Esc closes
  await page.keyboard.press("Shift+Tab");
  await expect(box.getByRole("button", { name: "Repository", exact: true })).not.toContainText(repo);
  await page.keyboard.press("Escape");
  await expect(box).toHaveCount(0);

  // a parent chosen in the box is the new item's parent
  await page.keyboard.press("c");
  await box.getByRole("button", { name: "Parent", exact: true }).click();
  await page.getByRole("option", { name: `${parent.key} Parent` }).click();
  await field.fill("Part of it");
  await page.keyboard.press("Enter");
  await expect(rows(page).filter({ hasText: "Part of it" })).toBeVisible();
  const items = (await (await request.get(`${core}/api/v1/items?repo=${encodeURIComponent(repo)}&q=Part`)).json()) as {
    items: { parent: string; created_by: string }[];
  };
  expect(items.items[0].parent).toBe(parent.key);
  expect(items.items[0].created_by).toBe("person");
});

test("J and K move, and S, D and X claim, close and cancel the focused item, but not while typing", async ({
  page,
  request,
}) => {
  const repo = fresh("keys");
  const first = await add(request, repo, "First");
  const second = await add(request, repo, "Second");
  await open(page, repo);
  await rows(page).first().hover();
  await expect(row(page, first.key)).toHaveClass(/is-focused/);
  await page.keyboard.press("j");
  await expect(row(page, second.key)).toHaveClass(/is-focused/);
  await page.keyboard.press("k");
  await expect(row(page, first.key)).toHaveClass(/is-focused/);

  await page.keyboard.press("s");
  await expect(row(page, first.key).getByRole("img", { name: "In progress" })).toBeVisible();
  await page.keyboard.press("s");
  await expect(row(page, first.key).getByRole("img", { name: "Open" })).toBeVisible();
  await page.keyboard.press("d");
  // done items leave the list of those not closed
  await expect(row(page, first.key)).toHaveCount(0);
  await expect(row(page, second.key)).toHaveClass(/is-focused/);
  await page.keyboard.press("x");
  await expect(row(page, second.key)).toHaveCount(0);
  await open(page, repo, "&status=all");
  await expect(row(page, first.key).getByRole("img", { name: "Done" })).toBeVisible();
  await expect(row(page, second.key).getByRole("img", { name: "Cancelled" })).toBeVisible();

  // typing in the search moves nothing
  await page.getByLabel("Search work items").fill("ds");
  await expect(row(page, first.key)).toHaveCount(0);
  await page.getByLabel("Search work items").fill("");
  await expect(row(page, first.key).getByRole("img", { name: "Done" })).toBeVisible();
});

test("an item an agent adds or changes shows without a reload", async ({ page, request }) => {
  const repo = fresh("live");
  await open(page, repo);
  await expect(page.getByText("No matching work items")).toBeVisible();
  const added = await add(request, repo, "From an agent");
  await expect(row(page, added.key)).toBeVisible();
  await edit(request, added.key, { claim: "claim" });
  await expect(row(page, added.key).getByRole("img", { name: "In progress" })).toBeVisible();
  await request.delete(`${core}/api/v1/items/${added.key}`, { headers: { origin: shellOrigin } });
  await expect(row(page, added.key)).toHaveCount(0);
});

test("the page shows its own changes without waiting for their event", async ({ page, request }) => {
  const repo = fresh("quiet");
  const item = await add(request, repo, "Quiet");
  // a window whose event stream carries nothing
  await page.route("**/api/v1/events*", (route) => route.abort());
  await open(page, repo);
  await row(page, item.key).hover();
  await page.keyboard.press("s");
  await expect(row(page, item.key).getByRole("img", { name: "In progress" })).toBeVisible();
  await page.keyboard.press("c");
  await page.getByLabel("What needs to be done?").fill("Added quietly");
  await page.keyboard.press("Enter");
  await expect(rows(page).filter({ hasText: "Added quietly" })).toBeVisible();
});

test("the sidebar counts the items the page lists when it opens, as they change", async ({ page, request }) => {
  const notClosed = async () =>
    ((await (await request.get(`${core}/api/v1/items`)).json()) as { items: unknown[] }).items.length;
  const before = await notClosed();
  await page.goto("/#/");
  const count = page.getByTestId("items-count");
  const shown = async () => Number((await count.count()) ? await count.textContent() : 0);
  await expect.poll(shown).toBe(before);
  const repo = fresh("count");
  const item = await add(request, repo, "Counted");
  await expect.poll(shown).toBe(before + 1);
  // in progress is still not closed
  await edit(request, item.key, { claim: "claim" });
  await page.waitForTimeout(300);
  expect(await shown()).toBe(before + 1);
  await edit(request, item.key, { status: "done" });
  await expect.poll(shown).toBe(before);
  // and the page, opened from it, lists as many
  await page
    .getByRole("navigation", { name: "Main" })
    .getByRole("link", { name: /Work items/ })
    .click();
  await expect(page.getByRole("heading", { name: "Work items", level: 1 })).toBeVisible();
  await expect(page.locator(".page-head .inbox-total")).toHaveText(String(await shown()));
});

test("with no items, the page says how to add one", async ({ page }) => {
  // the shard's core has other tests' items: this window is answered with none
  await page.route("**/api/v1/items*", (route) => route.fulfill({ json: { items: [] } }));
  await page.goto("/#/items");
  await expect(page.getByText("No work items yet")).toBeVisible();
  await expect(page.getByText("pinrail items add")).toBeVisible();
});

test("a filter that keeps nothing says so", async ({ page }) => {
  await open(page, fresh("empty"));
  await expect(page.getByText("No matching work items")).toBeVisible();
  await expect(page.getByText("Try a different search or clear the filters.")).toBeVisible();
});

test("what is typed as soon as the quick add opens is kept", async ({ page, request }) => {
  const repo = fresh("fast");
  const parent = await add(request, repo, "Parent");
  const part = await add(request, repo, "Part", { parent: parent.key });
  await add(request, repo, "Next", { parent: parent.key, depends_on: [part.key] });
  await edit(request, part.key, { claim: "claim" });
  const theme = (await (await request.get(`${core}/api/v1/settings`)).json()).appearance.theme;
  // C while the app re-renders for a change of theme and of nesting, then
  // typing at once: opening the field must not clear what is typed. Before
  // the fix, one round in three lost it.
  for (let round = 1; round <= 4; round++) {
    await request.patch(`${core}/api/v1/settings`, { data: { appearance: { theme: round % 2 ? "light" : "dark" } } });
    await open(page, repo);
    await expect(rows(page).first()).toBeVisible();
    await page.waitForTimeout(300);
    await page.getByRole("radio", { name: "Flat" }).click();
    await page.waitForTimeout(300);
    await page.getByRole("radio", { name: "Nested" }).click();
    await page.keyboard.press("c");
    const field = page.getByLabel("What needs to be done?");
    await field.fill(`Round ${round}`);
    await page.waitForTimeout(200);
    expect(await field.inputValue(), `round ${round}`).toBe(`Round ${round}`);
    await page.keyboard.press("Escape");
  }
  await request.patch(`${core}/api/v1/settings`, { data: { appearance: { theme } } });
});

test("each choice of the status filter shows its status's icon, and the trigger the chosen one's", async ({ page }) => {
  await open(page, fresh("icons"));
  const trigger = page.getByRole("button", { name: "Status", exact: true });
  await trigger.click();
  const listbox = page.getByRole("listbox", { name: "Status" });
  for (const [name, mark] of [
    ["Ready", ".item-status.is-open"],
    ["Blocked", ".item-status-blocked"],
    ["In progress", ".item-status.is-in_progress"],
    ["Not started", ".item-status.is-open"],
    ["Done", ".item-status.is-done"],
    ["Cancelled", ".item-status.is-cancelled"],
  ]) {
    // the icon says nothing the name does not: the option keeps its plain name
    const option = listbox.getByRole("option", { name, exact: true });
    await expect(option.locator(`.select-option-icon ${mark}`)).toBeVisible();
  }
  await listbox.getByRole("option", { name: "Done", exact: true }).click();
  await expect(trigger.locator(".select-icon .item-status.is-done")).toBeVisible();
});

test("an item dragged onto another becomes its sub-item, unless that closes a cycle", async ({ page, request }) => {
  const repo = fresh("drag");
  const parent = await add(request, repo, "Release");
  const child = await add(request, repo, "Notes");
  await open(page, repo);
  const drag = async (from: string, to: string, drop = true) => {
    const transfer = await page.evaluateHandle(() => new DataTransfer());
    await row(page, from).dispatchEvent("dragstart", { dataTransfer: transfer });
    await row(page, to).dispatchEvent("dragenter", { dataTransfer: transfer });
    await row(page, to).dispatchEvent("dragover", { dataTransfer: transfer });
    if (drop) await row(page, to).dispatchEvent("drop", { dataTransfer: transfer });
    await row(page, from).dispatchEvent("dragend", { dataTransfer: transfer });
  };
  // the row it would go under is marked while it is over it
  const transfer = await page.evaluateHandle(() => new DataTransfer());
  await row(page, child.key).dispatchEvent("dragstart", { dataTransfer: transfer });
  await row(page, parent.key).dispatchEvent("dragover", { dataTransfer: transfer });
  await expect(row(page, parent.key)).toHaveClass(/is-drop/);
  await row(page, child.key).dispatchEvent("dragend", { dataTransfer: transfer });
  await expect(row(page, parent.key)).not.toHaveClass(/is-drop/);

  await drag(child.key, parent.key);
  await expect(page.locator(".toast")).toContainText(`${child.key} is now part of ${parent.key}`);
  await expect(page.getByRole("button", { name: `Hide the sub-items of ${parent.key}`, exact: true })).toBeVisible();
  const item = (await (await request.get(`${core}/api/v1/items/${child.key}`)).json()) as { parent: string };
  expect(item.parent).toBe(parent.key);

  // the parent onto its own sub-item would make it wait for itself
  await drag(parent.key, child.key);
  await expect(page.locator(".toast").last()).toContainText(
    `this would make ${child.key} wait for itself: ${child.key} → ${parent.key} → ${child.key}`,
  );
  const unchanged = (await (await request.get(`${core}/api/v1/items/${parent.key}`)).json()) as { parent: unknown };
  expect(unchanged.parent).toBeNull();
});

test("cancelling an item with open sub-items asks first, and cancels them with it", async ({ page, request }) => {
  const repo = fresh("cancel");
  const parent = await add(request, repo, "Release");
  const open1 = await add(request, repo, "Notes", { parent: parent.key });
  const finished = await add(request, repo, "Changelog", { parent: parent.key });
  await edit(request, finished.key, { status: "done" });
  const alone = await add(request, repo, "Alone");
  await open(page, repo);

  // an item with no open sub-item is cancelled at once
  await row(page, alone.key).hover();
  await page.keyboard.press("x");
  await expect(row(page, alone.key)).toHaveCount(0);

  await row(page, parent.key).hover();
  await page.keyboard.press("x");
  const dialog = page.getByRole("dialog", { name: `Cancel ${parent.key}?` });
  await expect(dialog.getByTestId("cancel-item-subs")).toContainText("Its open sub-item is cancelled with it");
  await dialog.getByRole("button", { name: "Keep it" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(row(page, parent.key)).toBeVisible();

  await page.keyboard.press("x");
  await dialog.getByRole("button", { name: "Cancel both" }).click();
  await expect(row(page, parent.key)).toHaveCount(0);
  await expect(row(page, open1.key)).toHaveCount(0);
  const status = async (key: string) =>
    ((await (await request.get(`${core}/api/v1/items/${key}`)).json()) as { status: string }).status;
  expect(await status(open1.key)).toBe("cancelled");
  expect(await status(finished.key)).toBe("done");
});

test("the panel asks too before cancelling an item with open sub-items", async ({ page, request }) => {
  const repo = fresh("cancel-panel");
  const parent = await add(request, repo, "Release");
  const a = await add(request, repo, "Notes", { parent: parent.key });
  await add(request, repo, "Tag", { parent: parent.key });
  await open(page, repo, `&open=${parent.key}`);
  const panel = page.getByTestId("item-panel");
  await panel.getByRole("button", { name: "Status", exact: true }).click();
  await page.getByRole("option", { name: "Cancelled", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: `Cancel ${parent.key}?` });
  await expect(dialog.getByTestId("cancel-item-subs")).toContainText("Its 2 open sub-items are cancelled with it");
  await dialog.getByRole("button", { name: "Cancel them all" }).click();
  await expect(panel.getByRole("button", { name: "Status", exact: true })).toHaveText("Cancelled");
  // a sub-item's timeline says what took it
  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}&status=all&open=${a.key}`);
  await expect(panel.getByTestId("item-event").last()).toContainText(`cancelled it with ${parent.key}`);
});
