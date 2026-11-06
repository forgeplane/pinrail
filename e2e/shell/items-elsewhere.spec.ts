// Work items where other things name them: a review's chip in its header
// and its inbox row, a session thread's items, item keys in Markdown, a
// repository's key prefix on the Projects page, and the command palette.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core, onReviewPages } from "./helpers";

// a review opens on a page of its own, where its header is
onReviewPages();

let counter = 0;
const fresh = (name = "repo") => `ie-${Date.now()}-${++counter}/${name}`;

async function add(request: APIRequestContext, repo: string, title: string, extra: Record<string, unknown> = {}) {
  const response = await request.post(`${core}/api/v1/items`, {
    data: { repo, title, by: "claude-code", session: "s-else", ...extra },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { key: string; id: string };
}

async function review(request: APIRequestContext, repo: string, title: string, item?: string | null) {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "list",
      title,
      origin: { repo },
      requested_by: "claude-code",
      ...(item !== undefined ? { item } : {}),
      payload: { groups: [{ title: "g", items: [{ id: 1, title: "x" }] }] },
    },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { id: string };
}

const discard = (request: APIRequestContext, id: string) =>
  request.post(`${core}/api/v1/reviews/${id}/discard`, { data: { reason: "spec cleanup" } });

const panelKey = (page: Page) => page.getByTestId("item-panel").locator(".item-panel-key");

test("a review's chip in its inbox row and its header opens its item", async ({ page, request }) => {
  const repo = fresh("chips");
  const item = await add(request, repo, "Load themes");
  const sent = await review(request, repo, "Theme loader", item.key);
  await page.goto(`/#/?repo=${encodeURIComponent(repo)}`);
  const row = page.getByTestId("review-row").filter({ hasText: "Theme loader" });
  const chip = row.getByTestId("item-chip");
  await expect(chip).toContainText(`${item.key}Load themes`);
  // the chip opens the item, not the review
  await chip.click();
  await expect(page).toHaveURL(new RegExp(`#/items\\?open=${item.key}`));
  await expect(panelKey(page)).toHaveText(item.key);

  await page.goto(`/#/reviews/${sent.id}`);
  await page.getByTestId("review-strip").getByTestId("item-chip").click();
  await expect(panelKey(page)).toHaveText(item.key);
  await discard(request, sent.id);
});

test("a review is put under an item from its header, moved to another, and taken out", async ({ page, request }) => {
  const repo = fresh("link");
  const first = await add(request, repo, "First");
  const second = await add(request, repo, "Second");
  const sent = await review(request, repo, "Unfiled", null);
  await page.goto(`/#/reviews/${sent.id}`);
  const strip = page.getByTestId("review-strip");
  await expect(strip.getByTestId("item-chip")).toHaveCount(0);

  await strip.getByRole("button", { name: "Add to a work item", exact: true }).click();
  await page.getByRole("option", { name: `${first.key} First` }).click();
  await expect(strip.getByTestId("item-chip")).toContainText(first.key);
  await strip.getByRole("button", { name: "Change the work item", exact: true }).click();
  await page.getByRole("option", { name: `${second.key} Second` }).click();
  await expect(strip.getByTestId("item-chip")).toContainText(second.key);
  const listed = (await (await request.get(`${core}/api/v1/items/${second.key}`)).json()) as {
    reviews: { id: string }[];
  };
  expect(listed.reviews.map((r) => r.id)).toEqual([sent.id]);
  await strip.getByRole("button", { name: "Change the work item", exact: true }).click();
  await page.getByRole("option", { name: "No work item" }).click();
  await expect(strip.getByTestId("item-chip")).toHaveCount(0);
  await discard(request, sent.id);
});

test("a session's thread names the items it holds and worked on", async ({ page, request }) => {
  const repo = fresh("thread");
  const session = `s-thread-${Date.now()}`;
  await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { text: "Load the themes", kind: "start" },
  });
  const held = await add(request, repo, "Held", { session });
  const done = await add(request, repo, "Finished", { session });
  await add(request, repo, "Someone else's");
  await request.patch(`${core}/api/v1/items/${held.key}`, { data: { by: "claude-code", session, claim: "claim" } });
  await request.post(`${core}/api/v1/items/${done.key}/done`, { data: { by: "claude-code", session } });
  await page.goto(`/#/sessions/claude-code/${session}`);
  // the header names what it holds now
  await expect(page.getByTestId("thread-held-item")).toHaveText([`working on ${held.key}Held`]);
  // its details list every item it worked on, the one in progress first
  const items = page.getByTestId("thread-work-items");
  await expect(items).toContainText("Work items · 2");
  await expect(items.getByTestId("thread-work-item")).toHaveText([
    new RegExp(`Held${held.key} · in progress`),
    new RegExp(`Finished${done.key} · done`),
  ]);
  await items.getByTestId("thread-work-item").first().click();
  await expect(panelKey(page)).toHaveText(held.key);
});

test("an item's key in a comment, a note or a description links to it, an unknown prefix does not", async ({
  page,
  request,
}) => {
  const repo = fresh("keys");
  const target = await add(request, repo, "The target");
  const prefix = target.key.split("-")[0];
  const item = await add(request, repo, "Mentions", { body: `Depends on ${target.key}, unlike ZZZZ-1.` });
  await request.post(`${core}/api/v1/items/${item.key}/comments`, {
    data: { body: `Done after ${target.key}.`, by: "claude-code", session: "s-else" },
  });
  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}&open=${item.key}`);
  const panel = page.getByTestId("item-panel");
  const link = panel.locator(".item-md a.item-ref", { hasText: target.key });
  await expect(link.first()).toBeVisible();
  await expect(panel.locator('.item-md a[data-item="ZZZZ-1"]')).toHaveCount(0);
  await expect(panel.getByTestId("item-comment").locator(`a[data-item="${target.key}"]`)).toBeVisible();
  await link.first().click();
  await expect(panelKey(page)).toHaveText(target.key);

  // a status note links it too
  const session = `s-key-${Date.now()}`;
  await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { text: `Closed ${prefix}-${target.key.split("-")[1]} today.`, kind: "note" },
  });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const inNote = page.getByTestId("note").locator(`a[data-item="${target.key}"]`);
  await expect(inNote).toBeVisible();
  // inline with the text around it
  await expect(inNote).toHaveCSS("display", "inline");
  await inNote.click();
  await expect(panelKey(page)).toHaveText(target.key);
});

test("a repository's key prefix is changed from the Projects page, and its old keys still open its items", async ({
  page,
  request,
}) => {
  const repo = fresh("prefix");
  const other = fresh("taken");
  const item = await add(request, repo, "Keyed");
  const taken = (await add(request, other, "Elsewhere")).key.split("-")[0];
  const old = item.key;
  // a prefix no other test has: Q, two letters from the time, and X
  const letter = (n: number) => String.fromCharCode(65 + (Math.floor(n) % 26));
  const wanted = `Q${letter(Date.now() / 1000)}${letter(Date.now() / 26000)}X`;
  await page.goto("/#/projects");
  const rowOf = page.getByTestId("project-repo").filter({ hasText: repo });
  await rowOf.hover();
  await rowOf.getByRole("button", { name: `${repo} actions` }).click();
  await page.getByRole("menuitem", { name: "Change key prefix…" }).click();
  const dialog = page.getByRole("dialog", { name: `Key prefix of ${repo}` });
  await expect(dialog).toContainText(`keyed ${old.split("-")[0]}`);
  const field = dialog.getByRole("textbox");
  // a prefix another repository has is refused, with the reason
  await field.fill(taken);
  await dialog.getByRole("button", { name: "Change prefix" }).click();
  await expect(dialog.locator(".notice")).toContainText(`another repository has or had the prefix ${taken}`);
  // letters only, in capitals, as typed
  await field.fill(`${wanted.toLowerCase()}1`);
  await expect(field).toHaveValue(wanted);
  await dialog.getByRole("button", { name: "Change prefix" }).click();
  await expect(dialog).toHaveCount(0);

  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}`);
  await expect(page.getByTestId("item-row")).toHaveAttribute("data-key", `${wanted}-1`);
  // the old key still finds it, and the panel moves to the new one
  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}&open=${old}`);
  await expect(panelKey(page)).toHaveText(`${wanted}-1`);
  await expect(page).toHaveURL(new RegExp(`open=${wanted}-1`));
});

test("the command palette finds work items by key and title", async ({ page, request }) => {
  const repo = fresh("palette");
  const item = await add(request, repo, `Palette target ${Date.now()}`);
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  const field = palette.getByRole("textbox");
  for (const typed of [item.key, item.key.toLowerCase(), "Palette target"]) {
    await field.fill(typed);
    await expect(palette.getByText(/Palette target/).first()).toBeVisible();
  }
  await field.fill(item.key);
  await palette
    .getByText(/Palette target/)
    .first()
    .click();
  await expect(panelKey(page)).toHaveText(item.key);
});

test("a comment the session's agent writes on an item shows in its thread", async ({ page, request }) => {
  const repo = fresh("comment");
  const item = await add(request, repo, "Load themes");
  const session = `s-comment-${Date.now()}`;
  await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { text: "Loading themes", kind: "start", origin: { repo } },
  });
  const comment = (body: string) =>
    request.post(`${core}/api/v1/items/${item.key}/comments`, { data: { body, by: "claude-code", session } });
  expect((await comment("Themes load from `~/.pinrail/themes`.")).status()).toBe(201);
  // the person's comment is not the session's
  await request.post(`${core}/api/v1/items/${item.key}/comments`, {
    data: { body: "Prefer a watcher." },
    headers: { origin: "tauri://localhost" },
  });

  await page.goto(`/#/sessions/claude-code/${session}`);
  const cards = page.getByTestId("thread-comment");
  await expect(cards).toHaveCount(1);
  await expect(cards.first()).toContainText("claude-code commented on");
  await expect(cards.first().locator("code")).toHaveText("~/.pinrail/themes");
  // one written while the thread is open shows at once
  await comment("Done: they reload when a file changes.");
  await expect(cards).toHaveCount(2);
  await expect(cards.last()).toContainText("they reload when a file changes");

  await cards.first().getByTestId("item-chip").click();
  await expect(panelKey(page)).toHaveText(item.key);
  await expect(page.getByTestId("item-panel").getByTestId("item-comment")).toHaveCount(3);
});

test("a session's thread shows a line for each change its agent made to a work item", async ({ page, request }) => {
  const repo = fresh("events");
  const session = `s-events-${Date.now()}`;
  await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { text: "Loading themes", kind: "start", origin: { repo } },
  });
  const item = await add(request, repo, "Load themes", { session });
  await request.patch(`${core}/api/v1/items/${item.key}`, { data: { by: "claude-code", session, claim: "claim" } });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const lines = page.getByTestId("item-event");
  await expect(lines).toHaveText([/^Added.*Load themes/, /^Started working on.*Load themes/]);
  // each line's icon is centred on its text
  const offsets = await lines.evaluateAll((all) =>
    all.map((line) => {
      const icon = line.querySelector("svg.item-status")!.getBoundingClientRect();
      const text = line.querySelector(".thread-item-event")!.getBoundingClientRect();
      return Math.abs(icon.top + icon.height / 2 - (text.top + text.height / 2));
    }),
  );
  expect(Math.max(...offsets)).toBeLessThan(1);
  // one made while the thread is open shows at once
  await request.post(`${core}/api/v1/items/${item.key}/done`, { data: { by: "claude-code", session } });
  await expect(lines).toHaveCount(3);
  await expect(lines.last()).toContainText("Marked done");
  await lines.last().getByTestId("item-chip").click();
  await expect(panelKey(page)).toHaveText(item.key);
});
