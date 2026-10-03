// Developing a plugin with a link: a pending review follows the folder, a
// hand-over records the version the person saw, and the review keeps it
// once the link and the folder are gone.
import { expect, test, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { clearInbox, core, createReview, linkPlugin } from "./helpers";
import { scratch } from "../helpers/scratch";

const root = path.resolve(__dirname, "..", "..");

/** A copy of the hello plugin under a name of its own, without its tests. */
function helloCopy(name: string): string {
  const dir = scratch(`pinrail-${name}-`);
  fs.cpSync(path.join(root, "plugins", "hello"), dir, {
    recursive: true,
    filter: (src) => !/\/(tests|fixtures|node_modules)(\/|$)/.test(src),
  });
  const manifest = JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8"));
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ ...manifest, name }));
  return dir;
}

/** Adds a line to the view, as a developer's edit does. */
function edit(dir: string, text: string) {
  const page = path.join(dir, "view", "index.html");
  const html = fs.readFileSync(page, "utf8");
  fs.writeFileSync(page, html.replace("<body>", `<body>\n<p id="edited">${text}</p>`));
}

async function status(page: Page, id: string) {
  return (await (await page.request.get(`${core}/api/v1/reviews/${id}`)).json()).status as string;
}

test.beforeEach(async ({ page }) => {
  await clearInbox(page.request);
});

test("a pending review follows its linked folder, and keeps what was decided", async ({ page }) => {
  const dir = helloCopy("loop");
  await linkPlugin(page.request, dir, "loop");
  const { id } = await createReview(page.request, { plugin: "loop", title: "Loop", payload: { message: "Ship?" } });
  await page.goto(`/#/reviews/${id}`);
  const view = page.frameLocator("#plugin-frame");
  await expect(view.locator("#yes")).toBeVisible();

  // the folder changes: the app offers the new version, and Reload opens it
  edit(dir, "Edited view");
  await expect(page.locator("[data-plugin-newer]")).toBeVisible({ timeout: 10_000 });
  await page.locator("[data-plugin-reload]").click();
  await expect(view.locator("#edited")).toHaveText("Edited view");
  await expect(page.locator("[data-plugin-newer]")).toHaveCount(0);

  await view.locator("#yes").click();
  await page.locator("[data-handover]").click();
  await expect.poll(() => status(page, id)).toBe("decided");

  // the link and the folder go; the review shows what was decided with
  const removed = await page.request.delete(`${core}/api/v1/plugins/loop`);
  expect(removed.status()).toBe(200);
  fs.rmSync(dir, { recursive: true, force: true });
  await page.goto(`/#/reviews/${id}`);
  await expect(view.locator("#edited")).toHaveText("Edited view");
  await expect(view.locator("p").last()).toContainText("Decided: yes");
});

test("a review another window moved follows it here, and a conflict at hand-over opens it again", async ({ page }) => {
  const dir = helloCopy("stale");
  await linkPlugin(page.request, dir, "stale");
  const { id } = await createReview(page.request, { plugin: "stale", title: "Stale", payload: { message: "Ship?" } });
  await page.goto(`/#/reviews/${id}`);
  const view = page.frameLocator("#plugin-frame");
  await expect(view.locator("#yes")).toBeVisible();

  // another window opens the review after the folder changed: it moves, and
  // this one shows the version it moved to
  edit(dir, "Second version");
  const other = await page.request.post(`${core}/api/v1/reviews/${id}/view`, { data: {} });
  expect(other.status()).toBe(200);
  const moved = (await other.json()).bundle as string;
  await expect(view.locator("#edited")).toHaveText("Second version");

  // a hand-over the core refuses as a conflict, as one racing a move is:
  // nothing is decided, and the review opens again
  let refused = false;
  await page.route(`${core}/api/v1/reviews/${id}/decision`, (route) => {
    if (refused) return route.continue();
    refused = true;
    return route.fulfill({
      status: 409,
      contentType: "application/json",
      body: JSON.stringify({ error: "conflict", message: "moved", violations: [] }),
    });
  });
  await view.locator("#yes").click();
  await page.locator("[data-handover]").click();
  await expect(page.locator(".notice", { hasText: "opened again" })).toBeVisible();
  expect(await status(page, id)).toBe("pending");

  // decided again, once the view has opened again
  await expect(async () => {
    await view.locator("#yes").click();
    await page.locator("[data-handover]").click();
    await expect.poll(() => status(page, id), { timeout: 3_000 }).toBe("decided");
  }).toPass({ timeout: 15_000 });
  const decided = await (await page.request.get(`${core}/api/v1/reviews/${id}`)).json();
  expect(decided.plugin_bundle).toBe(moved);
  fs.rmSync(dir, { recursive: true, force: true });
});
