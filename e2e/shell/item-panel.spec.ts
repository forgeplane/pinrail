// A work item in the side panel: opening and closing it, editing it in
// place, its dependencies with their graph, sub-items, reviews, sessions,
// comments with the activity, and deleting it. Each test works in a
// repository of its own, since the shard's core is shared.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
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
  // marked by its background alone, as the inbox marks its open review
  await expect(row(page, first.key)).toHaveCSS("border-left-width", "0px");
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
  const body = panel(page).getByLabel("Description", { exact: true });
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
  await panel(page).getByRole("button", { name: "Add link", exact: true }).click();
  await panel(page).getByLabel("Link", { exact: true }).fill("https://github.com/forgeplane/pinrail/issues/9");
  await page.keyboard.press("Enter");
  const change = panel(page).getByRole("button", { name: "Change the link" });
  await expect(change).toHaveText("github.com/forgeplane/pinrail/issues/9");
  await change.click();
  await panel(page).getByLabel("Link", { exact: true }).fill("");
  await page.keyboard.press("Enter");
  await expect(panel(page).getByRole("button", { name: "Add link", exact: true })).toBeVisible();

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
      items: [item.key],
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

  await box.getByRole("button", { name: "Comment", exact: true }).click();
  const comments = panel(page).getByTestId("item-comment");
  await expect(comments).toHaveCount(1);
  await expect(comments.first()).toContainText("then the second");
  await expect(field).toHaveValue("");
});

test("the key, description and comments are text that can be selected and copied", async ({ page, request }) => {
  const repo = fresh("select");
  const item = await add(request, repo, "Selectable title", { body: "A description to copy." });
  await request.post(`${core}/api/v1/items/${item.key}/comments`, {
    data: { body: "A comment to copy.", by: "claude-code", session: "s-panel" },
  });
  await openItem(page, repo, item.key);
  const selected = async (target: ReturnType<typeof panel>) => {
    await target.click({ clickCount: 3 });
    return page.evaluate(() => window.getSelection()?.toString().trim());
  };
  await expect.poll(() => selected(panel(page).locator(".item-panel-key"))).toBe(item.key);
  await expect.poll(() => selected(panel(page).getByText("A description to copy."))).toBe("A description to copy.");
  await expect.poll(() => selected(panel(page).getByText("A comment to copy."))).toBe("A comment to copy.");
});

test("an image a comment carries is shown in place", async ({ page, request }) => {
  const repo = fresh("image");
  const item = await add(request, repo, "The dialog");
  const png = Buffer.from(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
    "base64",
  );
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

test("the properties are a row of pills under the title, with the creation line under them", async ({
  page,
  request,
}) => {
  const repo = fresh("pills");
  const item = await add(request, repo, "Item");
  await page.setViewportSize({ width: 1500, height: 800 });
  await openItem(page, repo, item.key);
  const pills = panel(page).getByRole("group", { name: "Properties" });
  const title = panel(page).locator(".item-panel-title");
  const titleBox = (await title.boundingBox())!;
  const status = (await pills.getByRole("button", { name: "Status", exact: true }).boundingBox())!;
  expect(status.y).toBeGreaterThan(titleBox.y + titleBox.height - 1);
  // status, claim, repository, parent and link on one line at this width
  for (const name of ["Claim", "Repository", "Parent", "Add link"]) {
    const at = (await pills.getByRole("button", { name, exact: true }).boundingBox())!;
    expect(Math.abs(at.y + at.height / 2 - (status.y + status.height / 2)), name).toBeLessThan(2);
  }
  const created = panel(page).getByTestId("item-created");
  await expect(created).toHaveText(/Added by claude-code · .+ ago$/);
  expect((await created.boundingBox())!.y).toBeGreaterThan(status.y + status.height);
});

// a 1×1 PNG
const PIXEL = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64",
);

test("files are attached to the description with the button or a paste, and removed from its list", async ({
  page,
  request,
}) => {
  const repo = fresh("attach");
  const item = await add(request, repo, "Item", { body: "Before." });
  await openItem(page, repo, item.key);
  await panel(page).getByRole("button", { name: "Edit" }).first().click();
  const body = panel(page).getByLabel("Description", { exact: true });
  await body.press("ControlOrMeta+End");

  // the button: an image goes in at the cursor, shown in place
  await panel(page)
    .locator('input[type="file"][aria-label="Attach a file to the description"]')
    .setInputFiles({ name: "latency chart.png", mimeType: "image/png", buffer: PIXEL });
  await expect(body).toHaveValue(/!\[latency-chart\.png\]\(attachment:latency-chart\.png\)/);

  // a paste: another file goes in as a link, under a name of its own when
  // the name is taken
  await body.evaluate((el) => {
    const data = new DataTransfer();
    data.items.add(new File(["line one\n"], "latency chart.png", { type: "image/png" }));
    data.items.add(new File(["log\n"], "run.log", { type: "text/plain" }));
    el.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
  });
  await expect(body).toHaveValue(/!\[latency-chart-2\.png\]\(attachment:latency-chart-2\.png\)/);
  await expect(body).toHaveValue(/\[run\.log\]\(attachment:run\.log\)/);
  await body.press("ControlOrMeta+Enter");

  await expect(panel(page).locator(".item-md img")).toHaveCount(2);
  const files = panel(page).locator(".item-files li");
  await expect(files).toHaveText(["latency-chart-2.png", "latency-chart.png", "run.log"]);
  const stored = await read(request, item.key);
  expect(stored.files.map((f: { name: string }) => f.name).sort()).toEqual([
    "latency-chart-2.png",
    "latency-chart.png",
    "run.log",
  ]);

  // removed from the list, a file leaves the item and its place in the text
  await panel(page).getByRole("button", { name: "Remove run.log" }).click();
  await expect(files).toHaveText(["latency-chart-2.png", "latency-chart.png"]);
  const after = await read(request, item.key);
  expect(after.body).not.toContain("attachment:run.log");
  expect(after.files.map((f: { name: string }) => f.name)).not.toContain("run.log");
});

test("a file is attached to a comment, dropped on its field", async ({ page, request }) => {
  const repo = fresh("comment-file");
  const item = await add(request, repo, "Item");
  await openItem(page, repo, item.key);
  const field = panel(page).getByLabel("Add a comment");
  await field.fill("The run: ");
  await field.evaluate((el) => {
    const data = new DataTransfer();
    data.items.add(new File(["log\n"], "run.log", { type: "text/plain" }));
    el.dispatchEvent(new DragEvent("drop", { dataTransfer: data, bubbles: true, cancelable: true }));
  });
  await expect(field).toHaveValue("The run: [run.log](attachment:run.log)");
  await field.press("ControlOrMeta+Enter");
  const card = panel(page).getByTestId("item-comment").filter({ hasText: "The run:" });
  await expect(card.locator(".item-md").getByRole("link", { name: "run.log" })).toBeVisible();
  const stored = await read(request, item.key);
  const comment = stored.timeline.find((e: { type: string }) => e.type === "comment");
  expect(comment.files.map((f: { name: string }) => f.name)).toEqual(["run.log"]);
});

test("a file attached to a comment takes a name no file of the item has", async ({ page, request }) => {
  const repo = fresh("comment-name");
  const sha256 = createHash("sha256").update(PIXEL).digest("hex");
  await request.put(`${core}/api/v1/attachments/${sha256}`, {
    data: PIXEL,
    headers: { "content-type": "application/octet-stream" },
  });
  const item = await add(request, repo, "Item", {
    body: "![image.png](attachment:image.png)",
    attachments: { "image.png": { sha256, size: PIXEL.length, media_type: "image/png" } },
  });
  await openItem(page, repo, item.key);
  const field = panel(page).getByLabel("Add a comment");
  await field.evaluate((el) => {
    const data = new DataTransfer();
    data.items.add(new File(["another picture"], "image.png", { type: "image/png" }));
    el.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
  });
  await expect(field).toHaveValue("![image-2.png](attachment:image-2.png)");
  await field.press("ControlOrMeta+Enter");
  await expect(panel(page).getByTestId("item-comment")).toHaveCount(1);
  const stored = await read(request, item.key);
  expect(stored.files.map((f: { sha256: string }) => f.sha256)).toEqual([sha256]);
});

test("two files pasted before the first is uploaded take names of their own", async ({ page, request }) => {
  const repo = fresh("quick-paste");
  const item = await add(request, repo, "Item");
  await openItem(page, repo, item.key);
  // the first upload takes a while
  let first = true;
  await page.route("**/api/v1/attachments/*", async (route) => {
    if (first) {
      first = false;
      await new Promise((resolve) => setTimeout(resolve, 800));
    }
    await route.continue();
  });
  const field = panel(page).getByLabel("Add a comment");
  const paste = (text: string) =>
    field.evaluate((el, text) => {
      const data = new DataTransfer();
      data.items.add(new File([text], "image.png", { type: "image/png" }));
      el.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
    }, text);
  await paste("the first picture");
  await paste("the second picture");
  await expect(field).toHaveValue(/attachment:image\.png\)/);
  await expect(field).toHaveValue(/attachment:image-2\.png\)/);
  await field.press("ControlOrMeta+Enter");
  await expect(panel(page).getByTestId("item-comment")).toHaveCount(1);
  const stored = await read(request, item.key);
  const comment = stored.timeline.find((e: { type: string }) => e.type === "comment");
  expect(comment.files.map((f: { name: string }) => f.name).sort()).toEqual(["image-2.png", "image.png"]);
});

test("the activity names a question or review in one line, its title as Markdown", async ({ page, request }) => {
  const repo = fresh("activity-reviews");
  const item = await add(request, repo, "Item");
  const sent = async (body: Record<string, unknown>) => {
    const response = await request.post(`${core}/api/v1/reviews`, {
      data: { origin: { repo }, requested_by: "claude-code", items: [item.key], ...body },
    });
    expect(response.status(), await response.text()).toBe(201);
    return ((await response.json()) as { id: string }).id;
  };
  const question = await sent({
    kind: "question",
    title: "Split `Thread.tsx` in two?",
    payload: { questions: [{ id: "q1", question: "Split it?", answer: "yes-no" }] },
  });
  await request.post(`${core}/api/v1/reviews/${question}/discard`, {
    data: { reason: "not now" },
    headers: { origin: shellOrigin },
  });
  const review = await sent({
    plugin: "list",
    title: "The plan",
    payload: { groups: [{ title: "g", items: [{ id: 1, title: "x" }] }] },
  });
  await request.post(`${core}/api/v1/reviews/${review}/withdraw`, { data: { reason: "replaced", by: "claude-code" } });

  await openItem(page, repo, item.key);
  // the run of changes is folded behind "Show … more"
  await panel(page)
    .getByRole("button", { name: /^Show \d+ more/ })
    .click();
  const lines = panel(page).getByTestId("item-event");
  await expect(lines.filter({ hasText: "Split" })).toHaveText([
    /claude-code asked Split Thread\.tsx in two\?/,
    /You declined Split Thread\.tsx in two\?/,
  ]);
  await expect(lines.filter({ hasText: "asked" }).locator("code")).toHaveText("Thread.tsx");
  await expect(lines.filter({ hasText: "asked" }).getByRole("link")).toHaveAttribute("href", `#/reviews/${question}`);
  await expect(lines.filter({ hasText: "The plan" })).toHaveText([
    /claude-code sent the review The plan/,
    /claude-code withdrew The plan/,
  ]);
});

test("a dialog keeps Tab inside it, and Esc gives the focus back to what opened it", async ({ page, request }) => {
  const repo = fresh("dialog-focus");
  const item = await add(request, repo, "Item");
  await openItem(page, repo, item.key);
  const opener = panel(page).getByRole("button", { name: "Delete…" });
  await opener.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: `Delete ${item.key}?` });
  await expect(dialog).toBeVisible();
  // Tab goes round the dialog's buttons and never leaves it
  for (let i = 0; i < 4; i++) {
    await page.keyboard.press("Tab");
    expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
  }
  await page.keyboard.press("Shift+Tab");
  expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(opener).toBeFocused();
});

test("a comment changed elsewhere opens its editor with the new text", async ({ page, request }) => {
  const repo = fresh("comment-stale");
  const item = await add(request, repo, "Item");
  await openItem(page, repo, item.key);
  const field = panel(page).getByLabel("Add a comment");
  await field.fill("First words.");
  await page.keyboard.press("ControlOrMeta+Enter");
  const comment = panel(page).getByTestId("item-comment");
  await expect(comment).toContainText("First words.");
  // another window changes it
  const detail = await (await request.get(`${core}/api/v1/items/${item.key}`)).json();
  const id = detail.timeline.find((e: { type: string }) => e.type === "comment").id;
  const changed = await request.patch(`${core}/api/v1/items/${item.key}/comments/${id}`, {
    data: { body: "Second words." },
    headers: { origin: shellOrigin },
  });
  expect(changed.status(), await changed.text()).toBe(200);
  await expect(comment).toContainText("Second words.");
  await comment.getByRole("button", { name: "Edit" }).click();
  await expect(comment.getByLabel("Comment")).toHaveValue("Second words.");
});
