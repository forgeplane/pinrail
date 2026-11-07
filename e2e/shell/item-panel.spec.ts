// A work item in the side panel: opening and closing it, editing it in
// place, its dependencies with their graph, sub-items, reviews, sessions,
// comments with the activity, and deleting it. Each test works in a
// repository of its own, since the shard's core is shared.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core, fakeRecognition, listening, shellOrigin, speak } from "./helpers";

let counter = 0;
const fresh = (name = "repo") => `ip-${Date.now()}-${++counter}/${name}`;

async function add(
  request: APIRequestContext,
  repo: string,
  title: string,
  extra: Record<string, unknown> = {},
  by = "claude-code",
) {
  const response = await request.post(`${core}/api/v1/items`, {
    data: { repo, title, by, session: "s-panel", ...extra },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { key: string; id: string };
}

async function read(request: APIRequestContext, key: string) {
  return (await (await request.get(`${core}/api/v1/items/${key}`)).json()) as Record<string, any>;
}

const panel = (page: Page) => page.getByTestId("item-panel");
const row = (page: Page, key: string) => page.locator(`[data-testid="item-row"][data-key="${key}"]`);

async function openItem(page: Page, repo: string, key: string) {
  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}&open=${key}`);
  await expect(panel(page).locator(".item-panel-key")).toHaveText(key);
}

test("an item opens in the panel from a click, Enter or its address, and Esc closes it", async ({ page, request }) => {
  const repo = fresh("open");
  const first = await add(request, repo, "First");
  const second = await add(request, repo, "Second");
  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}`);
  await row(page, first.key).click();
  await expect(panel(page).getByRole("heading", { name: "First" })).toBeVisible();
  await expect(row(page, first.key)).toHaveClass(/is-open/);
  await expect(page).toHaveURL(new RegExp(`open=${first.key}`));

  await page.keyboard.press("Escape");
  await expect(panel(page)).toHaveCount(0);
  await row(page, second.key).hover();
  await page.keyboard.press("Enter");
  await expect(panel(page).getByRole("heading", { name: "Second" })).toBeVisible();
  await panel(page).getByRole("button", { name: "Close" }).click();
  await expect(panel(page)).toHaveCount(0);

  await openItem(page, repo, first.key);
  await expect(panel(page).getByRole("heading", { name: "First" })).toBeVisible();
});

test("with the panel open, J and K open the item they move to", async ({ page, request }) => {
  const repo = fresh("walk");
  const first = await add(request, repo, "First");
  const second = await add(request, repo, "Second");
  const third = await add(request, repo, "Third");
  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}`);
  // with the panel closed, J only moves
  await page.locator("body").press("j");
  await expect(row(page, second.key)).toHaveClass(/is-focused/);
  await expect(panel(page)).toHaveCount(0);

  await row(page, first.key).click();
  await expect(panel(page).getByRole("heading", { name: "First" })).toBeVisible();
  await page.keyboard.press("j");
  await expect(panel(page).getByRole("heading", { name: "Second" })).toBeVisible();
  await expect(row(page, second.key)).toHaveClass(/is-open/);
  // the clicked row draws no focus ring once the keys have moved on
  await expect(row(page, first.key)).toHaveCSS("box-shadow", "none");
  await page.locator("body").press("j");
  await expect(panel(page).getByRole("heading", { name: "Third" })).toBeVisible();
  await expect(page).toHaveURL(new RegExp(`open=${third.key}`));
  await page.locator("body").press("k");
  await expect(panel(page).getByRole("heading", { name: "Second" })).toBeVisible();

  // a hovered row does not change where they move from
  await row(page, first.key).hover();
  await page.keyboard.press("j");
  await expect(panel(page).getByRole("heading", { name: "Third" })).toBeVisible();

  // an item opened from its address is where they move from
  await page.goto("/#/");
  await openItem(page, repo, third.key);
  await page.locator("body").press("k");
  await expect(panel(page).getByRole("heading", { name: "Second" })).toBeVisible();
});

test("the title and the description are edited in place, saved or left", async ({ page, request }) => {
  const repo = fresh("edit");
  const item = await add(request, repo, "Load themes", { body: "Read the files." });
  await openItem(page, repo, item.key);

  // T edits the title; Esc leaves it as it was
  await page.keyboard.press("t");
  const title = panel(page).getByLabel("Title");
  await expect(title).toBeFocused();
  await title.fill("Discarded");
  await page.keyboard.press("Escape");
  await expect(panel(page).getByRole("heading", { name: "Load themes" })).toBeVisible();
  // a click edits it too, and Enter saves
  await panel(page).getByRole("heading", { name: "Load themes" }).getByRole("button").click();
  await title.fill("Load themes in the core");
  await page.keyboard.press("Enter");
  await expect(panel(page).getByRole("heading", { name: "Load themes in the core" })).toBeVisible();
  await expect(row(page, item.key)).toContainText("Load themes in the core");
  expect((await read(request, item.key)).title).toBe("Load themes in the core");

  // E edits the description: Markdown source, saved with ⌘Enter
  await page.keyboard.press("e");
  const body = panel(page).getByLabel("Description");
  await expect(body).toHaveValue("Read the files.");
  await body.fill("Read the **theme** files.");
  await page.keyboard.press("ControlOrMeta+Enter");
  await expect(panel(page).locator(".item-md strong")).toHaveText("theme");
  // Esc leaves the description as it was
  await page.keyboard.press("e");
  await body.fill("Never kept");
  await page.keyboard.press("Escape");
  await expect(panel(page).locator(".item-md")).toContainText("Read the theme files.");
  expect((await read(request, item.key)).body).toBe("Read the **theme** files.");
});

test("status, claim, parent, repository and link are changed from the panel", async ({ page, request }) => {
  const repo = fresh("props");
  const elsewhere = fresh("elsewhere");
  const parent = await add(request, repo, "Parent");
  const item = await add(request, repo, "Item");
  await add(request, elsewhere, "Other repository");
  await openItem(page, repo, item.key);

  await panel(page).getByRole("button", { name: "Claim" }).click();
  await expect(panel(page).locator(".item-claimant")).toHaveText("by you");
  expect((await read(request, item.key)).claim).toEqual({ agent: "person", session: null });
  await panel(page).getByRole("button", { name: "Release" }).click();
  await expect(panel(page).getByRole("button", { name: "Claim" })).toBeVisible();

  const choose = async (label: string, option: string) => {
    await panel(page).getByRole("button", { name: label, exact: true }).click();
    await page.getByRole("option", { name: option, exact: true }).click();
  };
  await choose("Status", "Done");
  await expect.poll(async () => (await read(request, item.key)).status).toBe("done");
  await choose("Status", "Open");
  await choose("Parent", `${parent.key} Parent`);
  await expect(panel(page).getByRole("navigation", { name: "Part of" })).toContainText(parent.key);
  await choose("Parent", "No parent");
  await expect(panel(page).getByRole("navigation", { name: "Part of" })).toHaveCount(0);

  // a link is added, then removed with an empty one
  await panel(page).getByRole("button", { name: "Add", exact: true }).click();
  await panel(page).getByLabel("Link", { exact: true }).fill("https://github.com/forgeplane/pinrail/issues/9");
  await page.keyboard.press("Enter");
  await expect(panel(page).getByRole("link", { name: "https://github.com/forgeplane/pinrail/issues/9" })).toBeVisible();
  await panel(page).getByRole("button", { name: "Change" }).click();
  await panel(page).getByLabel("Link", { exact: true }).fill("");
  await page.keyboard.press("Enter");
  await expect(panel(page).locator(".item-link")).toContainText("None");

  // moved to another repository, it takes a new key, and the address follows
  await choose("Repository", elsewhere);
  await expect(panel(page).locator(".item-panel-key")).not.toHaveText(item.key);
  const moved = await panel(page).locator(".item-panel-key").textContent();
  await expect(page).toHaveURL(new RegExp(`open=${moved}`));
  expect((await read(request, item.key)).key).toBe(moved);
});

test("the graph shows what an item depends on and blocks, adds and removes, and refuses a cycle", async ({
  page,
  request,
}) => {
  const repo = fresh("deps");
  const done = await add(request, repo, "Theme file format");
  const dropped = await add(request, repo, "Settings schema");
  const item = await add(request, repo, "Load themes", { depends_on: [done.key, dropped.key] });
  const later = await add(request, repo, "Theme picker", { depends_on: [item.key] });
  const extra = await add(request, repo, "Extra");
  await request.post(`${core}/api/v1/items/${done.key}/done`, { data: { by: "codex", session: "x" } });
  await request.patch(`${core}/api/v1/items/${dropped.key}`, {
    data: { by: "codex", session: "x", status: "cancelled", comment: "Replaced" },
  });
  await openItem(page, repo, item.key);

  const graph = panel(page).getByTestId("dependency-graph");
  const left = graph.getByLabel("Depends on");
  const right = graph.getByLabel("Blocks");
  await expect(left.getByRole("button", { name: new RegExp(done.key) })).toBeVisible();
  await expect(left.getByRole("img", { name: "Done" })).toBeVisible();
  await expect(left.getByRole("img", { name: "Cancelled" })).toBeVisible();
  await expect(right.getByRole("button", { name: new RegExp(later.key) })).toBeVisible();
  await expect(panel(page).getByTestId("cancelled-dependency")).toHaveText(
    `${dropped.key} was cancelled: the work it was to do will not happen.`,
  );

  // added from the picker
  await panel(page).getByRole("button", { name: "Add a dependency", exact: true }).click();
  await page.getByRole("option", { name: `${extra.key} Extra` }).click();
  await expect(left.getByRole("button", { name: new RegExp(extra.key) })).toBeVisible();
  // removed with its node's button
  await left.getByRole("button", { name: new RegExp(extra.key) }).hover();
  await left.getByRole("button", { name: `No longer depend on ${extra.key}` }).click();
  await expect(left.getByRole("button", { name: new RegExp(extra.key) })).toHaveCount(0);

  // a cycle is refused, with its circle, and nothing changes
  await panel(page).getByRole("button", { name: "Add a dependency", exact: true }).click();
  await page.getByRole("option", { name: `${later.key} Theme picker` }).click();
  await expect(page.locator(".toast")).toContainText(
    `this would make ${item.key} wait for itself: ${item.key} → ${later.key} → ${item.key}`,
  );
  expect((await read(request, item.key)).blocked_by).toEqual([]);

  // a node opens its item
  await right.getByRole("button", { name: new RegExp(later.key) }).click();
  await expect(panel(page).locator(".item-panel-key")).toHaveText(later.key);
});

test("sub-items are listed, opened and added from the panel", async ({ page, request }) => {
  const repo = fresh("subs");
  const parent = await add(request, repo, "Release");
  const child = await add(request, repo, "Notes", { parent: parent.key });
  await openItem(page, repo, parent.key);
  const subs = panel(page).getByRole("region", { name: "Sub-items" });
  await expect(subs.getByRole("button", { name: new RegExp(`${child.key} Notes`) })).toBeVisible();
  await subs.getByLabel("Add a sub-item").fill("Tag the release");
  await page.keyboard.press("Enter");
  await expect(subs.getByRole("button", { name: /Tag the release/ })).toBeVisible();
  await expect(subs.getByLabel("Add a sub-item")).toHaveValue("");
  const added = (await read(request, parent.key)).sub_items_list.find((s: any) => s.title === "Tag the release");
  expect((await read(request, added.key)).created_by).toBe("person");
  await subs.getByRole("button", { name: new RegExp(`${child.key} Notes`) }).click();
  await expect(panel(page).locator(".item-panel-key")).toHaveText(child.key);
  await expect(panel(page).getByRole("navigation", { name: "Part of" })).toContainText(parent.key);
});

test("the reviews and sessions of an item are listed and lead to them", async ({ page, request }) => {
  const repo = fresh("links");
  const item = await add(request, repo, "Item");
  const sent = await request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "list",
      title: "Theme loader",
      origin: { repo },
      requested_by: "claude-code",
      session: "s-panel",
      item: item.key,
      payload: { groups: [{ title: "g", items: [{ id: 1, title: "x" }] }] },
    },
  });
  const review = (await sent.json()) as { id: string };
  await openItem(page, repo, item.key);
  const reviews = panel(page).getByRole("region", { name: "Reviews" });
  await expect(reviews.getByRole("link", { name: /Theme loader/ })).toContainText("pending");
  await expect(panel(page).getByRole("region", { name: "Sessions" })).toContainText("s-panel");
  await reviews.getByRole("link", { name: /Theme loader/ }).click();
  await expect(page).toHaveURL(new RegExp(review.id));
  // the inbox is shared with the other tests
  await request.post(`${core}/api/v1/reviews/${review.id}/discard`, { data: { reason: "spec cleanup" } });
});

test("the person comments, an agent's comment shows live, and the activity reads in order", async ({
  page,
  request,
}) => {
  const repo = fresh("comments");
  const item = await add(request, repo, "Watch the folder");
  await request.patch(`${core}/api/v1/items/${item.key}`, {
    data: { by: "claude-code", session: "s-panel", claim: "claim" },
  });
  await openItem(page, repo, item.key);
  const field = panel(page).getByLabel("Add a comment");
  await field.fill("Prefer a file **watcher**.");
  await page.keyboard.press("ControlOrMeta+Enter");
  const comments = panel(page).getByTestId("item-comment");
  await expect(comments).toHaveCount(1);
  await expect(comments.first().locator("strong")).toHaveText("watcher");
  await expect(field).toHaveValue("");

  await request.post(`${core}/api/v1/items/${item.key}/comments`, {
    data: { body: "Renames are missed on Linux.", by: "claude-code", session: "s-panel" },
  });
  await expect(comments).toHaveCount(2);
  await expect(comments.nth(1)).toContainText("claude-code");
  await expect(comments.nth(1).getByRole("button", { name: "Edit" })).toHaveCount(0);
  const events = panel(page).getByTestId("item-event");
  await expect(events).toContainText(["claude-code added it", "claude-code started working on it"]);

  // the person's own comment is edited and deleted
  await comments.first().getByRole("button", { name: "Edit" }).click();
  await comments.first().getByLabel("Comment").fill("Prefer a watcher.");
  await page.keyboard.press("ControlOrMeta+Enter");
  await expect(comments.first()).toContainText("Prefer a watcher.");
  await expect(comments.first()).toContainText("edited");
  await comments.first().getByRole("button", { name: "Delete" }).click();
  await expect(comments).toHaveCount(1);
  await expect(comments.first()).toContainText("Renames are missed on Linux.");
});

test("the comment field is the chat box: Enter starts a new line, it is dictated into, and its arrow posts", async ({
  page,
  request,
}) => {
  await fakeRecognition(page);
  const repo = fresh("compose");
  const item = await add(request, repo, "Dictated comment");
  await openItem(page, repo, item.key);
  const field = panel(page).getByLabel("Add a comment");
  // one box, as in a session's thread, with no frame of the text's own
  const box = panel(page)
    .locator(".compose")
    .filter({ has: page.getByLabel("Add a comment") });
  await expect(box).toBeVisible();
  await expect(field).toHaveCSS("border-top-width", "0px");

  await field.fill("First line");
  await field.press("Enter");
  await field.pressSequentially("then ");
  await expect(field).toHaveValue("First line\nthen ");
  await expect(panel(page).getByTestId("item-comment")).toHaveCount(0);

  await box.getByRole("button", { name: "Dictate" }).click();
  await expect.poll(() => listening(page)).toBe(true);
  await speak(page, "the second");
  await expect(field).toHaveValue("First line\nthen the second");
  await box.getByRole("button", { name: "Stop dictating" }).click();

  await box.getByRole("button", { name: "Comment" }).click();
  const comments = panel(page).getByTestId("item-comment");
  await expect(comments).toHaveCount(1);
  await expect(comments.first()).toContainText("then the second");
  await expect(field).toHaveValue("");
});

test("an image a comment carries is shown in place", async ({ page, request }) => {
  const repo = fresh("image");
  const item = await add(request, repo, "The dialog");
  const png = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
    "base64",
  );
  const { createHash } = await import("node:crypto");
  const hash = createHash("sha256").update(png).digest("hex");
  await request.put(`${core}/api/v1/attachments/${hash}`, {
    data: png,
    headers: { "content-type": "application/octet-stream" },
  });
  await request.post(`${core}/api/v1/items/${item.key}/comments`, {
    data: {
      body: "The error: ![the dialog](attachment:dialog.png)",
      // as the CLI declares a file: its media type from its name
      attachments: { "dialog.png": { sha256: hash, size: png.length, media_type: "image/png" } },
      by: "claude-code",
      session: "s-panel",
    },
  });
  await openItem(page, repo, item.key);
  const image = panel(page).getByTestId("item-comment").getByRole("img", { name: "the dialog" });
  await expect(image).toBeVisible();
  await expect.poll(() => image.evaluate((img) => (img as HTMLImageElement).naturalWidth)).toBe(1);
});

test("deleting an item asks first, says how many sub-items go, and closes the panel", async ({ page, request }) => {
  const repo = fresh("delete");
  const parent = await add(request, repo, "Parent");
  await add(request, repo, "Child", { parent: parent.key });
  await openItem(page, repo, parent.key);
  await panel(page).getByRole("button", { name: "Delete…" }).click();
  const dialog = page.getByRole("dialog", { name: `Delete ${parent.key}?` });
  await expect(dialog.getByTestId("delete-item-subs")).toHaveText("Its sub-item goes with it, and theirs too.");
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(row(page, parent.key)).toBeVisible();
  await panel(page).getByRole("button", { name: "Delete…" }).click();
  await dialog.getByRole("button", { name: "Delete item" }).click();
  await expect(panel(page)).toHaveCount(0);
  await expect(page.locator('[data-testid="item-row"]')).toHaveCount(0);
});

test("an item is copied as Markdown", async ({ page, request, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  const repo = fresh("copy");
  const item = await add(request, repo, "Load themes", { body: "Read the files." });
  await openItem(page, repo, item.key);
  await panel(page).getByRole("button", { name: "Copy as Markdown" }).click();
  await expect
    .poll(() => page.evaluate(() => navigator.clipboard.readText()))
    .toBe(`# ${item.key} Load themes\n\nRead the files.\n`);
});

test("an item deleted elsewhere says so in the panel", async ({ page, request }) => {
  const repo = fresh("gone");
  const item = await add(request, repo, "Soon gone");
  await openItem(page, repo, item.key);
  await request.delete(`${core}/api/v1/items/${item.key}`, { headers: { origin: shellOrigin } });
  await expect(panel(page)).toContainText("This work item is no longer here.");
});
