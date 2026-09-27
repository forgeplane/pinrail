// A plugin's view cannot open links by itself: it asks the app, and the app
// asks the person, unless the person allowed that origin for that plugin.

import { expect, test, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { clearInbox, core, createReview, linkPlugin } from "./helpers";
import { scratch } from "../helpers/scratch";

const WEB = "https://example.com/page?x=1";
const LONG = `https://example.com/?data=${"x".repeat(2100)}`;

/** A view whose buttons ask the app to open links, and one that tries to
 *  allow its own links through its settings. */
function opener(): string {
  const dir = scratch("pinrail-opener-");
  fs.mkdirSync(path.join(dir, "view"));
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({ name: "opener", version: "1.0.0", title: "Opener", entry: "view/index.html", payload_schema: {}, decision_schema: {} }),
  );
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html><meta charset="utf-8"><script src="/sdk/v1/pinrail-plugin.js"></script>
<button id="web">web</button><button id="other">other</button><button id="long">long</button>
<button id="mail">mail</button><button id="burst">burst</button><button id="grant">grant</button>
<script>
  const plugin = Pinrail.connect({ onInit() {} });
  const on = (id, fn) => document.getElementById(id).addEventListener("click", fn);
  on("web", () => plugin.open(${JSON.stringify(WEB)}));
  on("other", () => plugin.open("https://other.example/"));
  on("long", () => plugin.open(${JSON.stringify(LONG)}));
  on("mail", () => plugin.open("mailto:someone@example.com?body=hello"));
  on("burst", () => { for (let i = 1; i <= 3; i++) plugin.open("https://example.com/?n=" + i); });
  // what a hostile view would send, without the SDK
  on("grant", () => window.parent.postMessage({ pinrail: 1, type: "settings_set", patch: { links: { opener: { source: "x", origins: ["https://example.com"] } } } }, "*"));
</script>`,
  );
  return dir;
}

/** Records what the app would open in the system browser, instead of opening it. */
async function recordOpens(page: Page) {
  await page.addInitScript(() => {
    (window as unknown as { opened: string[] }).opened = [];
    window.open = ((url: string) => {
      (window as unknown as { opened: string[] }).opened.push(String(url));
      return null;
    }) as typeof window.open;
  });
}
const opened = (page: Page) => page.evaluate(() => (window as unknown as { opened: string[] }).opened);

const links = async (page: Page) => ((await (await page.request.get(`${core}/api/v1/settings`)).json()).links ?? {}) as Record<string, { source: string; origins: string[] }>;

let dir: string;
test.beforeAll(async ({ request }) => {
  dir = opener();
  await linkPlugin(request, dir, "opener");
});

test.beforeEach(async ({ page }) => {
  await recordOpens(page);
  await page.request.patch(`${core}/api/v1/settings`, { data: { links: { opener: null } } });
  await clearInbox(page.request);
  const { id } = await createReview(page.request, { plugin: "opener", title: "Links", payload: {} });
  await page.goto(`/#/reviews/${id}`);
  await expect(page.frameLocator("#plugin-frame").locator("#web")).toBeVisible();
});

const view = (page: Page) => page.frameLocator("#plugin-frame");
const dialog = (page: Page) => page.locator("[data-link-dialog]");

test("a link opens only when the person agrees, and requests made while asking are dropped", async ({ page }) => {
  await view(page).locator("#burst").click();
  await expect(dialog(page)).toBeVisible();
  await expect(dialog(page).locator("[data-link-target]")).toHaveText("example.com");
  await expect(dialog(page)).toContainText("Opener");
  expect(await opened(page)).toEqual([]);

  await dialog(page).locator("[data-link-cancel]").click();
  await expect(dialog(page)).toHaveCount(0);
  await page.waitForTimeout(300);
  expect(await opened(page)).toEqual([]);
});

test("Open once opens it, and the next request is asked about again", async ({ page }) => {
  await view(page).locator("#web").click();
  await dialog(page).locator("[data-link-once]").click();
  await expect.poll(() => opened(page)).toEqual([WEB]);

  await view(page).locator("#web").click();
  await expect(dialog(page)).toBeVisible();
  expect(await opened(page)).toEqual([WEB]);
});

test("Always allows that origin for that plugin, and only that origin", async ({ page }) => {
  await view(page).locator("#web").click();
  await dialog(page).locator("[data-link-always]").click();
  await expect.poll(() => opened(page)).toEqual([WEB]);
  await expect.poll(async () => (await links(page)).opener?.origins).toEqual(["https://example.com"]);

  // the same origin opens without asking
  await view(page).locator("#web").click();
  await expect.poll(() => opened(page)).toEqual([WEB, WEB]);
  await expect(dialog(page)).toHaveCount(0);

  // another origin is asked about
  await view(page).locator("#other").click();
  await expect(dialog(page).locator("[data-link-target]")).toHaveText("other.example");
  await dialog(page).locator("[data-link-cancel]").click();

  // a very long address is asked about even on an allowed origin, and cannot be allowed for good
  await view(page).locator("#long").click();
  await expect(dialog(page)).toBeVisible();
  await expect(dialog(page).locator("[data-link-always]")).toHaveCount(0);
});

test("an email is always asked about, and cannot be allowed for good", async ({ page }) => {
  await view(page).locator("#mail").click();
  await expect(dialog(page)).toContainText("Write an email?");
  await expect(dialog(page).locator("[data-link-target]")).toHaveText("someone@example.com");
  await expect(dialog(page).locator("[data-link-always]")).toHaveCount(0);
  await dialog(page).locator("[data-link-once]").click();
  await expect.poll(() => opened(page)).toEqual(["mailto:someone@example.com?body=hello"]);
});

test("a view cannot allow its own links", async ({ page }) => {
  await view(page).locator("#grant").click();
  await page.waitForTimeout(300);
  expect(await links(page)).toEqual({});
  await view(page).locator("#web").click();
  await expect(dialog(page)).toBeVisible();
});

test("Settings lists the origins a plugin opens without asking, and one can be removed", async ({ page }) => {
  await view(page).locator("#web").click();
  await dialog(page).locator("[data-link-always]").click();
  await expect.poll(async () => (await links(page)).opener?.origins).toEqual(["https://example.com"]);

  await page.goto("/#/plugins");
  const row = page.locator('[data-plugin-links="opener"]');
  await expect(row).toContainText("https://example.com");
  await row.locator('[data-forget-link="https://example.com"]').click();
  await expect(row).toHaveCount(0);
  await expect.poll(async () => links(page)).toEqual({});
});

test("a permission given to the plugin from another source does not apply", async ({ page }) => {
  // as after the plugin was removed and installed again from somewhere else
  const saved = await page.request.patch(`${core}/api/v1/settings`, { data: { links: { opener: { source: "https://github.com/someone/else", origins: ["https://example.com"] } } } });
  expect(saved.status(), await saved.text()).toBe(200);
  await page.reload();
  await expect(view(page).locator("#web")).toBeVisible();
  await view(page).locator("#web").click();
  await expect(dialog(page)).toBeVisible();
  expect(await opened(page)).toEqual([]);
});
