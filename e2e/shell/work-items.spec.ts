// The Work items page: rows, grouping, nested and flat, the filters, the
// quick add, the keys and live updates. Every test works in repositories of
// its own and opens the page filtered to them, since the shard's core is
// shared.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core, settingSaved, settingsDialog, settingsSection, shellOrigin } from "./helpers";

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
/** Starts dragging an item's row, as the browser's drag and drop does, and
 *  returns the transfer and a function that ends the drag. The drag ends on
 *  the row that was picked up, held on to: an item moved to another
 *  repository has a new key, and the page may show it in its new place
 *  before the drag ends. */
async function dragRow(page: Page, key: string) {
  const transfer = await page.evaluateHandle(() => new DataTransfer());
  const picked = await row(page, key).elementHandle();
  await picked!.dispatchEvent("dragstart", { dataTransfer: transfer });
  return { transfer, end: () => picked!.dispatchEvent("dragend", { dataTransfer: transfer }) };
}
const keys = (page: Page) => rows(page).evaluateAll((els) => els.map((e) => e.getAttribute("data-key")));

/** Chooses `option` in a submenu of the Filter or the Display menu, and
 *  closes the menu. */
async function pick(page: Page, menu: "Filter" | "Display options", submenu: string, option: string) {
  await page.getByRole("button", { name: menu, exact: true }).click();
  await page.getByRole("menuitem", { name: new RegExp(`^${submenu}`) }).click();
  await page.getByRole("menuitemradio", { name: option, exact: true }).click();
  await page.keyboard.press("Escape");
}

/** The page, filtered to one repository, or to a project with `project:`. */
async function open(page: Page, scope: string, extra = "") {
  const param = scope.startsWith("project:") ? `project=${scope.slice(8)}` : `repo=${encodeURIComponent(scope)}`;
  await page.goto(`/#/items?${param}${extra}`);
  await expect(page.getByRole("heading", { name: "Work items", level: 1 })).toBeVisible();
}

// the layout and the nesting are settings the core keeps: each test leaves
// the defaults for the next
test.afterEach(async ({ request }) => {
  await request.patch(`${core}/api/v1/settings`, {
    data: { items: { layout: "project", nesting: "nested", ordering: "oldest" } },
  });
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
      items: [reviewed.key],
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

  await pick(page, "Display options", "Sub-items", "Flat");
  await settingSaved(request, (s) => s.items.nesting, "flat");
  await expect(row(page, child.key).locator(".item-parent")).toHaveText(`in ${parent.key}`);
  expect(await indent(child.key)).toBe(await indent(parent.key));
  // the choice is kept
  await page.reload();
  await page.getByRole("button", { name: "Display options" }).click();
  await expect(page.getByRole("menuitem", { name: /^Sub-items/ })).toContainText("Flat");
  await page.keyboard.press("Escape");
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

  await pick(page, "Display options", "Grouping", "Repository");
  await settingSaved(request, (s) => s.items.layout, "repository");
  await expect(headings).toHaveCount(2);
  await expect(row(page, a.key).locator(".item-repo")).toHaveCount(0);

  await pick(page, "Display options", "Grouping", "None");
  await settingSaved(request, (s) => s.items.layout, "list");
  await expect(headings).toHaveCount(0);
  await expect(row(page, w.key).locator(".item-repo")).toHaveText(web);
  await page.reload();
  await page.getByRole("button", { name: "Display options" }).click();
  await expect(page.getByRole("menuitem", { name: /^Grouping/ })).toContainText("None");
  await page.keyboard.press("Escape");
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

  // by default the list shows the items open or in progress, which it calls
  // open, and no chip says so: only the repository is a filter
  const chips = page.getByTestId("filter-chip");
  await expect(chips).toHaveCount(1);
  await page.getByRole("button", { name: "Filter", exact: true }).click();
  await expect(page.getByRole("menuitem", { name: /^Status/ })).toContainText("Open");
  await page.keyboard.press("Escape");
  const choose = (label: string, option: string) => pick(page, "Filter", label, option);
  await choose("Status", "Ready");
  await expect(chips.filter({ hasText: "Status" })).toHaveText("Status: Ready");
  await expect.poll(() => keys(page)).toEqual([ready.key, mine.key, personKey]);
  await choose("Status", "Blocked");
  await expect.poll(() => keys(page)).toEqual([blocked.key]);
  await choose("Status", "Done");
  await expect.poll(() => keys(page)).toEqual([done.key]);
  await choose("Status", "Open");
  // back to the default, the status chip goes
  await expect(chips.filter({ hasText: "Status" })).toHaveCount(0);
  await choose("Created by", "You");
  await expect.poll(() => keys(page)).toEqual([personKey]);
  // a chip removes its filter
  await chips.filter({ hasText: "Created by" }).getByRole("button", { name: "Remove the Created by filter" }).click();
  await expect.poll(() => keys(page)).toEqual([ready.key, blocked.key, mine.key, personKey]);
  await page.getByRole("button", { name: "Filter", exact: true }).click();
  await page.getByRole("menuitemcheckbox", { name: "Waiting on you" }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByText("No matching work items")).toBeVisible();
  await expect(chips).toHaveText(["Project or repository: " + repo, "Waiting on you"]);
  await page.getByRole("button", { name: "Clear filters" }).click();
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
  // the box's accent border frames the field, which has no border of its own
  const border = (el: typeof field) =>
    el.evaluate((e) => {
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
  await expect.poll(() => border(box)).toEqual({ width: "1px", color: accent });
  expect((await border(field)).width).toBe("0px");
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
  // a click outside the field leaves it
  await page.getByRole("heading", { name: "Work items", level: 1 }).click();
  await expect(page.getByLabel("Search work items")).not.toBeFocused();
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
  // in progress is still not closed, and is counted on its own too
  const working = page.getByTestId("items-in-progress");
  const inProgress = async () => Number((await working.count()) ? await working.textContent() : 0);
  const workingBefore = await inProgress();
  await edit(request, item.key, { claim: "claim" });
  await expect.poll(inProgress).toBe(workingBefore + 1);
  expect(await shown()).toBe(before + 1);
  await edit(request, item.key, { status: "done" });
  await expect.poll(shown).toBe(before);
  await expect.poll(inProgress).toBe(workingBefore);
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
    await request.patch(`${core}/api/v1/settings`, { data: { items: { nesting: "flat" } } });
    await page.waitForTimeout(300);
    await request.patch(`${core}/api/v1/settings`, { data: { items: { nesting: "nested" } } });
    await page.keyboard.press("c");
    const field = page.getByLabel("What needs to be done?");
    await field.fill(`Round ${round}`);
    await page.waitForTimeout(200);
    expect(await field.inputValue(), `round ${round}`).toBe(`Round ${round}`);
    await page.keyboard.press("Escape");
  }
  await request.patch(`${core}/api/v1/settings`, { data: { appearance: { theme } } });
});

test("each choice of the status filter shows its status's icon, and its chip the chosen one's", async ({ page }) => {
  await open(page, fresh("icons"));
  await page.getByRole("button", { name: "Filter", exact: true }).click();
  await page.getByRole("menuitem", { name: /^Status/ }).click();
  const listbox = page.getByRole("menu", { name: "Status" });
  for (const [name, mark] of [
    ["Ready", ".item-status.is-open"],
    ["Blocked", ".item-status-blocked"],
    ["In progress", ".item-status.is-in_progress"],
    ["Not started", ".item-status.is-open"],
    ["Done", ".item-status.is-done"],
    ["Cancelled", ".item-status.is-cancelled"],
  ]) {
    // the icon says nothing the name does not: the option keeps its plain name
    const option = listbox.getByRole("menuitemradio", { name, exact: true });
    await expect(option.locator(`.menu-mark ${mark}`)).toBeVisible();
  }
  await listbox.getByRole("menuitemradio", { name: "Done", exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(
    page.getByTestId("filter-chip").filter({ hasText: "Status" }).locator(".item-status.is-done"),
  ).toBeVisible();
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

test("a sub-item dragged to the top of the list leaves its parent", async ({ page, request }) => {
  const repo = fresh("unparent");
  const parent = await add(request, repo, "Release");
  const child = await add(request, repo, "Notes", { parent: parent.key });
  const alone = await add(request, repo, "Alone");
  await open(page, repo);
  const top = page.getByTestId("item-top-drop");
  // only a sub-item, while it is dragged, can go to the top level
  await expect(top).toHaveCount(0);
  let transfer = await page.evaluateHandle(() => new DataTransfer());
  await row(page, alone.key).dispatchEvent("dragstart", { dataTransfer: transfer });
  await expect(top).toHaveCount(0);
  await row(page, alone.key).dispatchEvent("dragend", { dataTransfer: transfer });

  transfer = await page.evaluateHandle(() => new DataTransfer());
  await row(page, child.key).dispatchEvent("dragstart", { dataTransfer: transfer });
  await expect(top).toHaveText("Drop here to move it to the top level");
  await top.dispatchEvent("dragover", { dataTransfer: transfer });
  await expect(top).toHaveClass(/is-drop/);
  await top.dispatchEvent("drop", { dataTransfer: transfer });
  await row(page, child.key).dispatchEvent("dragend", { dataTransfer: transfer });
  await expect(top).toHaveCount(0);
  await expect(page.locator(".toast")).toContainText(`${child.key} is no longer part of ${parent.key}`);
  const item = (await (await request.get(`${core}/api/v1/items/${child.key}`)).json()) as { parent: unknown };
  expect(item.parent).toBeNull();
  await expect(page.getByRole("button", { name: `Hide the sub-items of ${parent.key}`, exact: true })).toHaveCount(0);
});

test("a sub-item drags with the mouse, onto another item and to the top level", async ({ page, request }) => {
  const repo = fresh("mouse");
  const parent = await add(request, repo, "Release");
  const child = await add(request, repo, "Notes", { parent: parent.key });
  const other = await add(request, repo, "Other");
  await open(page, repo);
  /** Drags a row with the mouse, as a person does, and drops it on what `to` finds once the drag is on. */
  const drag = async (from: string, to: () => ReturnType<typeof row>) => {
    const start = (await row(page, from).boundingBox())!;
    await page.mouse.move(start.x + 40, start.y + start.height / 2);
    await page.mouse.down();
    await page.mouse.move(start.x + 60, start.y + start.height / 2 + 10, { steps: 5 });
    const target = (await to().boundingBox())!;
    await page.mouse.move(target.x + target.width / 2, target.y + target.height / 2, { steps: 10 });
    await page.mouse.up();
  };
  await drag(child.key, () => row(page, other.key));
  await expect(page.locator(".toast")).toContainText(`${child.key} is now part of ${other.key}`);
  // the list shows it under its new parent before it is dragged again
  await expect(page.getByRole("button", { name: `Hide the sub-items of ${other.key}`, exact: true })).toBeVisible();
  await drag(child.key, () => page.getByTestId("item-top-drop"));
  await expect(page.locator(".toast").last()).toContainText(`${child.key} is no longer part of ${other.key}`);
  const item = (await (await request.get(`${core}/api/v1/items/${child.key}`)).json()) as { parent: unknown };
  expect(item.parent).toBeNull();
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

test("an item with an open sub-item cannot be marked done, and says which", async ({ page, request }) => {
  const repo = fresh("notdone");
  const parent = await add(request, repo, "Release");
  const child = await add(request, repo, "Notes", { parent: parent.key });
  await open(page, repo);
  await row(page, parent.key).hover();
  await expect(row(page, parent.key)).toHaveClass(/is-focused/);
  await page.keyboard.press("d");
  await expect(page.locator(".toast").last()).toContainText(
    `${parent.key} is not done: its sub-item ${child.key} is still open`,
  );
  await expect(row(page, parent.key).getByRole("img", { name: "Open" })).toBeVisible();
  // once its last sub-item is done, the parent is done with it
  await row(page, child.key).hover();
  await page.keyboard.press("d");
  await expect(row(page, child.key)).toHaveCount(0);
  await expect(row(page, parent.key)).toHaveCount(0);
});

test("the items are ordered as Display options says, sub-items under their parent too, and the choice is kept", async ({
  page,
  request,
}) => {
  const repo = fresh("order");
  const zulu = await add(request, repo, "Zulu");
  const alpha = await add(request, repo, "Alpha");
  const child2 = await add(request, repo, "Yankee", { parent: alpha.key });
  const child1 = await add(request, repo, "Bravo", { parent: alpha.key });
  await edit(request, zulu.key, { claim: "claim" });
  await open(page, repo);
  expect(await keys(page)).toEqual([zulu.key, alpha.key, child2.key, child1.key]);

  await pick(page, "Display options", "Ordering", "Title");
  await settingSaved(request, (s) => s.items.ordering, "title");
  await expect.poll(() => keys(page)).toEqual([alpha.key, child1.key, child2.key, zulu.key]);
  await pick(page, "Display options", "Ordering", "Newest first");
  await expect.poll(() => keys(page)).toEqual([alpha.key, child1.key, child2.key, zulu.key]);
  await pick(page, "Display options", "Ordering", "Status");
  // in progress first
  await expect.poll(() => keys(page)).toEqual([zulu.key, alpha.key, child2.key, child1.key]);
  await page.reload();
  await page.getByRole("button", { name: "Display options" }).click();
  await expect(page.getByRole("menuitem", { name: /^Ordering/ })).toContainText("Status");
});

test("a submenu stays open while the pointer crosses another row on its way to it", async ({ page }) => {
  await page.goto("/#/items");
  await page.getByRole("button", { name: "Display options", exact: true }).click();
  const grouping = page.getByRole("menuitem", { name: /^Grouping/ });
  const ordering = page.getByRole("menuitem", { name: /^Ordering/ });
  await grouping.hover();
  const none = page.getByRole("menuitemradio", { name: "None", exact: true });
  await expect(none).toBeVisible();
  // from the right of the Grouping row to the submenu's last option, on
  // the left and below, the pointer crosses the Ordering row
  const [from, over, to] = [await grouping.boundingBox(), await ordering.boundingBox(), await none.boundingBox()];
  const start = { x: from!.x + from!.width - 8, y: from!.y + from!.height / 2 };
  const end = { x: to!.x + to!.width / 2, y: to!.y + to!.height / 2 };
  const crossesAt = start.x + ((over!.y + over!.height / 2 - start.y) / (end.y - start.y)) * (end.x - start.x);
  expect(crossesAt).toBeGreaterThan(over!.x);
  await page.mouse.move(start.x, start.y);
  await page.mouse.move(end.x, end.y, { steps: 12 });
  await expect(none).toBeVisible();
  await none.click();
  await settingSaved(page.request, (s) => s.items.layout, "list");
  await page.keyboard.press("Escape");

  // a pointer that stops on the row it crossed opens that row's submenu
  await page.getByRole("button", { name: "Display options", exact: true }).click();
  await grouping.hover();
  await expect(none).toBeVisible();
  await ordering.hover();
  await expect(page.getByRole("menuitemradio", { name: "Newest first", exact: true })).toBeVisible();
});

test("an item dragged to another group's heading moves to its repository", async ({ page, request }) => {
  const api = fresh("api");
  const web = api.replace("/api", "/web");
  const created = await request.post(`${core}/api/v1/projects`, {
    data: { name: `Moves ${Date.now()}`, repos: [api, web] },
  });
  const project = (await created.json()) as { id: string };
  const moved = await add(request, api, "Moves to the site");
  await add(request, web, "Stays in the site");
  await request.patch(`${core}/api/v1/settings`, { data: { items: { layout: "repository" } } });
  await open(page, `project:${project.id}`);
  const heading = (repo: string) => page.getByTestId("inbox-group-heading").filter({ hasText: repo });
  const { transfer, end } = await dragRow(page, moved.key);
  // the other group's heading lights up, and its own does not
  await expect(heading(web)).toHaveClass(/is-drop-target/);
  await expect(heading(api)).not.toHaveClass(/is-drop-target/);
  await heading(web).dispatchEvent("dragover", { dataTransfer: transfer });
  await expect(heading(web)).toHaveClass(/is-drop/);
  await heading(web).dispatchEvent("drop", { dataTransfer: transfer });
  await end();
  await expect(page.locator(".toast").last()).toContainText(`${moved.key} moved to ${web}`);
  const item = (await (await request.get(`${core}/api/v1/items/${moved.key}`)).json()) as { repo: string };
  expect(item.repo).toBe(web);
});

test("grouped by project, a drop asks which repository of a project with several", async ({ page, request }) => {
  const run = `${Date.now()}`;
  const one = fresh("one");
  const [api, web] = [fresh("api"), fresh("web")];
  const single = (await (
    await request.post(`${core}/api/v1/projects`, { data: { name: `Single ${run}`, repos: [one] } })
  ).json()) as { id: string };
  const several = (await (
    await request.post(`${core}/api/v1/projects`, { data: { name: `Several ${run}`, repos: [api, web] } })
  ).json()) as { id: string };
  const moved = await add(request, one, `Moves ${run}`);
  await add(request, one, `Stays in one ${run}`);
  await add(request, api, `Stays ${run}`);
  await page.goto(`/#/items`);
  await page.locator(".work-items input[type=search]").fill(run);
  const heading = (name: string) => page.getByTestId("inbox-group-heading").filter({ hasText: name });
  await expect(heading(`Several ${run}`)).toBeVisible();

  let drag = await dragRow(page, moved.key);
  // a person drops on a heading they can see
  await heading(`Several ${run}`).scrollIntoViewIfNeeded();
  await heading(`Several ${run}`).dispatchEvent("drop", { dataTransfer: drag.transfer });
  await drag.end();
  const menu = page.getByRole("menu", { name: "Move to which repository?" });
  await expect(menu.getByRole("menuitem")).toHaveText([api, web]);
  await expect(menu).toBeInViewport({ ratio: 1 });
  await menu.getByRole("menuitem", { name: web }).click();
  await expect(page.locator(".toast").last()).toContainText(`${moved.key} moved to ${web}`);
  let item = (await (await request.get(`${core}/api/v1/items/${moved.key}`)).json()) as { repo: string; key: string };
  expect(item.repo).toBe(web);

  // a project with one repository takes it there at once
  drag = await dragRow(page, item.key);
  await heading(`Single ${run}`).dispatchEvent("drop", { dataTransfer: drag.transfer });
  await drag.end();
  await expect(page.locator(".toast").last()).toContainText(`moved to ${one}`);
  // back in its first repository it has a new number; its key in the second still finds it
  item = (await (await request.get(`${core}/api/v1/items/${item.key}`)).json()) as { repo: string; key: string };
  expect(item.repo).toBe(one);
  expect(single.id).toBeTruthy();
  expect(several.id).toBeTruthy();
});

test("an item dropped on an item of another repository moves there and becomes its sub-item", async ({
  page,
  request,
}) => {
  const api = fresh("api");
  const web = api.replace("/api", "/web");
  const created = await request.post(`${core}/api/v1/projects`, {
    data: { name: `Nests ${Date.now()}`, repos: [api, web] },
  });
  const project = (await created.json()) as { id: string };
  const moved = await add(request, api, "Nests in the site");
  const parent = await add(request, web, "The site's release");
  await open(page, `project:${project.id}`);
  const { transfer, end } = await dragRow(page, moved.key);
  await row(page, parent.key).dispatchEvent("drop", { dataTransfer: transfer });
  await end();
  await expect(page.locator(".toast").last()).toContainText(`is now part of ${parent.key}`);
  const item = (await (await request.get(`${core}/api/v1/items/${moved.key}`)).json()) as {
    repo: string;
    parent: string | null;
  };
  expect(item.repo).toBe(web);
  expect(item.parent).toBe(parent.key);
});

test("beside the side panel, the quick add keeps its title field wide and its controls inside it", async ({
  page,
  request,
}) => {
  const repo = fresh("forgeplane-pinrail-with-a-long-name");
  const item = await add(request, repo, "Load themes");
  await page.setViewportSize({ width: 1300, height: 700 });
  await open(page, repo, `&open=${item.key}`);
  await expect(page.getByTestId("item-panel")).toBeVisible();
  await page.getByTestId("item-quick-add").click();
  const box = page.getByTestId("item-quick-box");
  const field = box.getByRole("textbox", { name: "What needs to be done?" });
  await expect(field).toBeFocused();
  expect((await field.boundingBox())!.width).toBeGreaterThanOrEqual(200);
  const outer = (await box.boundingBox())!;
  for (const control of await box.getByRole("button").all()) {
    const at = (await control.boundingBox())!;
    expect(at.x).toBeGreaterThanOrEqual(outer.x);
    expect(at.x + at.width, await control.getAttribute("aria-label")).toBeLessThanOrEqual(outer.x + outer.width);
  }
  await page.screenshot({ path: test.info().outputPath("quick-add-beside-panel.png") });
});

test("a narrow quick add keeps its controls on the second line whatever repository is chosen", async ({
  page,
  request,
}) => {
  const stamp = Date.now();
  const short = `s${stamp % 1000}/a`;
  const long = `q${stamp}/a-repository-with-a-much-longer-name`;
  await add(request, short, "Short");
  await add(request, long, "Long");
  // a box wide enough for the field and the controls of a short name on
  // one line, but not for those of a long one
  await page.setViewportSize({ width: 820, height: 700 });
  await page.goto(`/#/items?repo=${encodeURIComponent(short)}`);
  await page.getByTestId("item-quick-add").click();
  const box = page.getByTestId("item-quick-box");
  const field = box.getByRole("textbox", { name: "What needs to be done?" });
  const cancel = box.getByRole("button", { name: "Cancel" });
  const below = async () => (await cancel.boundingBox())!.y > (await field.boundingBox())!.y + 10;
  // the controls stay under the field for a short name and a long one
  for (const repo of [short, long, short]) {
    await box.getByRole("button", { name: "Repository" }).click();
    await page.getByRole("option", { name: repo, exact: true }).click();
    await expect(box.getByRole("button", { name: "Repository" })).toContainText(repo.slice(0, 8));
    expect(await below(), repo).toBe(true);
  }
});

test("beside the side panel, the list keeps at least 560px", async ({ page, request }) => {
  const repo = fresh("wide");
  const item = await add(request, repo, "Load themes");
  // a panel dragged wide on another page
  await page.addInitScript(() => localStorage.setItem("pinrail.inbox.panel", "900"));
  await page.setViewportSize({ width: 1300, height: 700 });
  await open(page, repo, `&open=${item.key}`);
  await expect(page.getByTestId("work item-panel")).toBeVisible();
  const list = page.locator(".work-items-side");
  expect((await list.boundingBox())!.width).toBeGreaterThanOrEqual(560);

  // dragging the edge stops there too
  const edge = page.getByRole("separator", { name: "Resize the work item panel" });
  const at = (await edge.boundingBox())!;
  await page.mouse.move(at.x + at.width / 2, at.y + at.height / 2);
  await page.mouse.down();
  await page.mouse.move(300, at.y + at.height / 2, { steps: 5 });
  await page.mouse.up();
  expect((await list.boundingBox())!.width).toBeGreaterThanOrEqual(560);
});

test("H puts the focused item on hold and takes it off, and the status filter has On hold", async ({
  page,
  request,
}) => {
  const repo = fresh("hold");
  const first = await add(request, repo, "First");
  const second = await add(request, repo, "Second");
  await open(page, repo, "&status=all");
  await row(page, first.key).hover();
  await page.keyboard.press("h");
  await expect(row(page, first.key).getByRole("img", { name: "On hold" })).toBeVisible();
  await expect
    .poll(
      async () =>
        ((await (await request.get(`${core}/api/v1/items/${first.key}`)).json()) as { status: string }).status,
    )
    .toBe("on_hold");

  // Open shows it with the others, and On hold shows it alone
  await open(page, repo);
  await expect(row(page, first.key).getByRole("img", { name: "On hold" })).toBeVisible();
  await expect(row(page, second.key)).toBeVisible();
  await open(page, repo, "&status=on_hold");
  await expect(rows(page)).toHaveCount(1);
  await row(page, first.key).hover();
  await page.keyboard.press("h");
  await expect(row(page, first.key)).toHaveCount(0);

  // the panel's status menu offers it too, and the activity says so
  await open(page, repo, `&open=${second.key}`);
  await page.getByTestId("item-panel").getByRole("button", { name: "Status", exact: true }).click();
  await page.getByRole("option", { name: "On hold", exact: true }).click();
  await expect(page.getByTestId("item-panel").getByTestId("item-event").last()).toContainText("put it on hold");
});

test("Q queues the focused item, its row shows its place, and ⇧↑ and ⇧↓ move it", async ({ page, request }) => {
  const repo = fresh("queue");
  const first = await add(request, repo, "First");
  const second = await add(request, repo, "Second");
  const third = await add(request, repo, "Third");
  // the queue is one for all repositories: start from an empty one
  const before = (await (await request.get(`${core}/api/v1/items?queued=true`)).json()) as {
    items: { key: string }[];
  };
  for (const i of before.items)
    await request.delete(`${core}/api/v1/items/${i.key}/queue`, { headers: { origin: shellOrigin } });
  // a review waits, so that the keys that go to waiting reviews would act
  const waiting = await request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "list",
      title: "Waits while the queue is changed",
      origin: { repo },
      requested_by: "claude-code",
      payload: { groups: [{ title: "g", items: [{ id: 1, title: "x" }] }] },
    },
  });
  expect(waiting.status()).toBe(201);
  const review = ((await waiting.json()) as { id: string }).id;
  await open(page, repo);
  const place = (key: string) => row(page, key).getByTestId("item-queued");
  await expect(page.getByTestId("item-queued")).toHaveCount(0);

  await row(page, first.key).hover();
  await page.keyboard.press("q");
  await row(page, third.key).hover();
  await page.keyboard.press("q");
  await expect(place(first.key)).toHaveText("1");
  await expect(place(third.key)).toHaveText("2");
  await expect(place(second.key)).toHaveCount(0);
  // the rows keep their places in the list
  expect(await keys(page)).toEqual([first.key, second.key, third.key]);

  // ⇧↑ and ⇧↓ move the focused item in the queue; ⌥↑ and ⌥↓ stay the
  // keys that go to the waiting reviews
  await page.keyboard.press("Shift+ArrowUp");
  await expect(place(third.key)).toHaveText("1");
  await expect(place(first.key)).toHaveText("2");
  await page.keyboard.press("Shift+ArrowDown");
  await expect(place(third.key)).toHaveText("2");

  // Q again takes it out
  await row(page, first.key).hover();
  await page.keyboard.press("q");
  await expect(place(first.key)).toHaveCount(0);
  await expect(place(third.key)).toHaveText("1");

  // the panel's pill queues the open item and names its place
  await open(page, repo, `&open=${second.key}`);
  const pill = page.getByTestId("item-panel").getByRole("button", { name: /^Queue/ });
  await expect(pill).toHaveText("Queue");
  await pill.click();
  await expect(pill).toHaveText("Queued · 2nd");
  await expect(place(second.key)).toHaveText("2");
  await request.post(`${core}/api/v1/reviews/${review}/discard`, { data: { reason: "spec cleanup" } });
});

test("the list loads again for item changes, not for a session's statuses", async ({ page, request }) => {
  const repo = fresh("notices");
  const item = await add(request, repo, "Listed");
  await open(page, repo);
  await expect(row(page, item.key)).toBeVisible();
  let loads = 0;
  page.on("request", (r) => {
    if (new URL(r.url()).pathname === "/api/v1/items") loads += 1;
  });
  for (const text of ["Reading", "Testing", "Writing"]) {
    await request.post(`${core}/api/v1/sessions/claude-code/s-notices/status`, { data: { text } });
  }
  // time for a reload to start, had a status caused one
  await page.waitForTimeout(500);
  expect(loads).toBe(0);

  await edit(request, item.key, { title: "Listed, renamed" });
  await expect(row(page, item.key)).toContainText("Listed, renamed");
  expect(loads).toBeGreaterThan(0);
});

test("H on an item with work in progress under it asks first", async ({ page, request }) => {
  const repo = fresh("hold-ask");
  const parent = await add(request, repo, "Release");
  const child = await add(request, repo, "Notes", { parent: parent.key });
  await edit(request, child.key, { claim: "claim" }, "s-holding");
  const status = async (key: string) =>
    ((await (await request.get(`${core}/api/v1/items/${key}`)).json()) as { status: string }).status;
  await open(page, repo);
  const dialog = page.getByRole("dialog", { name: `Put ${parent.key} on hold?` });

  await row(page, parent.key).hover();
  await page.keyboard.press("h");
  await expect(dialog).toContainText(`${child.key} Notes`);
  await expect(dialog).toContainText("claude-code is working on it");
  await dialog.getByRole("button", { name: "Keep working" }).click();
  await expect(dialog).toHaveCount(0);
  expect(await status(parent.key)).toBe("open");

  // the panel's status menu asks too
  await open(page, repo, `&open=${parent.key}`);
  await page.getByTestId("item-panel").getByRole("button", { name: "Status", exact: true }).click();
  await page.getByRole("option", { name: "On hold", exact: true }).click();
  await expect(dialog).toContainText(`${child.key} Notes`);
  await dialog.getByRole("button", { name: "Put on hold" }).click();
  await expect.poll(() => status(parent.key)).toBe("on_hold");
  expect(await status(child.key)).toBe("open");
});

test("a row's fold button is not inside the row's button, and J focuses the row", async ({ page, request }) => {
  const repo = fresh("a11y");
  const parent = await add(request, repo, "Parent");
  await add(request, repo, "Child", { parent: parent.key });
  await open(page, repo);
  const fold = page.getByRole("button", { name: `Hide the sub-items of ${parent.key}` });
  await expect(fold).toBeVisible();
  expect(await fold.evaluate((el) => el.parentElement?.closest('[role="button"]') === null)).toBe(true);
  await page.locator("body").press("j");
  await expect(page.locator(".item-row.is-focused")).toBeFocused();
});

test("a parent shows the work under way under it and how much is closed", async ({ page, request }) => {
  const repo = fresh("under-way");
  const parent = await add(request, repo, "Release");
  const notes = await add(request, repo, "Notes", { parent: parent.key });
  const tag = await add(request, repo, "Tag", { parent: parent.key });
  await add(request, repo, "Announce", { parent: parent.key });
  await edit(request, notes.key, { claim: "claim" });
  await request.post(`${core}/api/v1/items/${tag.key}/done`, {
    data: { by: "claude-code", session: "s-shell", comment: "Tagged" },
  });
  await open(page, repo, "&nesting=flat");
  const parentRow = row(page, parent.key);
  await expect(parentRow.getByTestId("item-active")).toHaveText(/1 in progress/);
  await expect(parentRow.getByTestId("item-progress")).toHaveText("1/3");
  await expect(parentRow.locator(".progress-ring")).toBeVisible();
  await parentRow.click();
  await expect(page.getByTestId("item-sub-progress")).toHaveText("1 of 3 closed · 1 in progress");
});

test("Settings has a Work items page with the switch that closes parents", async ({ page, request }) => {
  await page.goto("/#/items");
  await page.keyboard.press("ControlOrMeta+,");
  await expect(settingsDialog(page)).toBeVisible();
  await settingsSection(page, "Work items").click();
  const close = settingsDialog(page).getByRole("switch", { name: "Close a parent with its last sub-item" });
  await expect(close).toBeChecked();
  await close.click();
  await settingSaved(request, (s) => s.items.close_parents, false);
  await close.click();
  await settingSaved(request, (s) => s.items.close_parents, true);
});

test("J and K wrap around at the ends of the list", async ({ page, request }) => {
  const repo = fresh("wrap");
  const first = await add(request, repo, "First");
  const second = await add(request, repo, "Second");
  await open(page, repo);
  await expect(rows(page)).toHaveCount(2);
  const lit = page.locator(".item-row.is-focused");
  await expect(lit).toHaveAttribute("data-key", first.key);
  await page.locator("body").press("k");
  await expect(lit).toHaveAttribute("data-key", second.key);
  await page.locator("body").press("j");
  await expect(lit).toHaveAttribute("data-key", first.key);
});

test("with an item open in the panel, D acts on that item, even once it leaves the list", async ({ page, request }) => {
  const repo = fresh("panelkeys");
  const a = await add(request, repo, "Alpha");
  const b = await add(request, repo, "Bravo");
  const status = async (key: string) =>
    ((await (await request.get(`${core}/api/v1/items/${key}`)).json()) as { status: string }).status;
  await open(page, repo, `&open=${a.key}`);
  await expect(page.getByTestId("item-panel")).toContainText("Alpha");
  await page.locator("body").press("d");
  await expect.poll(() => status(a.key)).toBe("done");
  await expect(row(page, a.key)).toHaveCount(0);
  // the panel still shows Alpha: D again reopens it, and Bravo is left alone
  await page.locator("body").press("d");
  await expect.poll(() => status(a.key)).toBe("open");
  expect(await status(b.key)).toBe("open");
});

test("⌘⌫ deletes the open or the focused item, after asking, and leaves a text field alone", async ({
  page,
  request,
}) => {
  const repo = fresh("deletekey");
  const a = await add(request, repo, "Alpha");
  const b = await add(request, repo, "Bravo");
  await open(page, repo, `&open=${a.key}`);
  await expect(page.getByTestId("item-panel")).toContainText("Alpha");

  // in a text field, the key is the field's
  await page.getByLabel("Search work items").fill("Al");
  await page.getByLabel("Search work items").press("ControlOrMeta+Backspace");
  await expect(page.getByRole("dialog", { name: /^Delete / })).toHaveCount(0);
  await page.getByLabel("Search work items").fill("");
  // a click outside the field leaves it
  await page.getByRole("heading", { name: "Work items", level: 1 }).click();
  await expect(page.getByLabel("Search work items")).not.toBeFocused();

  // the item open in the panel
  await page.keyboard.press("ControlOrMeta+Backspace");
  const dialog = page.getByRole("dialog", { name: `Delete ${a.key}?` });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Delete item" }).click();
  await expect(row(page, a.key)).toHaveCount(0);
  await expect(page.getByTestId("item-panel")).toHaveCount(0);

  // the focused row
  await row(page, b.key).hover();
  await expect(row(page, b.key)).toHaveClass(/is-focused/);
  await page.keyboard.press("ControlOrMeta+Backspace");
  await page
    .getByRole("dialog", { name: `Delete ${b.key}?` })
    .getByRole("button", { name: "Delete item" })
    .click();
  await expect(row(page, b.key)).toHaveCount(0);
});
