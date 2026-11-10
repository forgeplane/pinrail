// What the shell specs share: where the core listens, and reviews submitted,
// decided and cleared through its API, as an agent or the app would.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";

/** Which of the runs side by side this is, from SHELL_SHARD (0 when the
 *  suite runs alone): each has its own ports and data folder. */
export const shard = Number(process.env.SHELL_SHARD ?? 0);
/** The port of the headless core the shell runs against. */
export const corePort = 4799 + 10 * shard;
export const core = `http://127.0.0.1:${corePort}`;
/** The port the shell is served on. */
export const uiPort = 5199 + 10 * shard;
/** The origin the shell runs on in these tests, which the core takes for the app's. */
export const shellOrigin = `http://127.0.0.1:${uiPort}`;

/** A list review with one proposal, for when the content does not matter. */
export const onePayload = {
  summary: "One proposal.",
  groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }],
};

type Review = {
  title: string;
  plugin?: string;
  origin?: Record<string, string>;
  payload?: unknown;
  revises?: string;
};

/** Submits a review as an agent would: a list with one proposal unless told otherwise. */
export async function createReview(
  request: APIRequestContext,
  { title, plugin = "list", origin = { repo: "acme/api" }, payload = onePayload, revises }: Review,
): Promise<{ id: string }> {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: { plugin, title, origin, requested_by: "spec", payload, revises },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { id: string };
}

/** Records a decision as the app does when a person submits one: from the
 *  shell's origin, so the core records it as made in the app. */
export async function decide(request: APIRequestContext, id: string, data: unknown) {
  const response = await request.post(`${core}/api/v1/reviews/${id}/decision`, {
    data: { data },
    headers: { origin: shellOrigin },
  });
  expect(response.status(), await response.text()).toBe(200);
}

/** Sets where the inbox opens a review: in its side panel (the default) or on a page. */
export async function openReviewsIn(request: APIRequestContext, where: "panel" | "page") {
  const response = await request.patch(`${core}/api/v1/settings`, { data: { inbox: { open_in: where } } });
  expect(response.status(), await response.text()).toBe(200);
}

/** Opens reviews on a page for each test in the spec, and restores the default after it. */
export function onReviewPages() {
  test.beforeEach(({ request }) => openReviewsIn(request, "page"));
  test.afterEach(({ request }) => openReviewsIn(request, "panel"));
}

/** Discards whatever is pending, so a test starts from an empty inbox. */
/** Waits until the core has a setting the page changed. The app saves a
 *  change in the background, so a test that ends right after one would
 *  leave the save to land in the next test, which the live updates then
 *  show on its page. */
export async function settingSaved(
  request: APIRequestContext,
  read: (settings: Record<string, any>) => unknown,
  expected: unknown,
) {
  await expect.poll(async () => read(await (await request.get(`${core}/api/v1/settings`)).json())).toEqual(expected);
}

export async function clearInbox(request: APIRequestContext) {
  const pending = (await (await request.get(`${core}/api/v1/reviews?status=pending&limit=500`)).json()).reviews as {
    id: string;
  }[];
  for (const r of pending) {
    const response = await request.post(`${core}/api/v1/reviews/${r.id}/discard`, { data: { reason: "spec cleanup" } });
    expect(response.status(), `discarding ${r.id}: ${await response.text()}`).toBe(200);
  }
}

/** The settings dialog. */
export const settingsDialog = (page: Page) => page.getByRole("dialog", { name: "Settings", exact: true });

/** A section's button in the settings dialog's rail, such as "Data". */
export const settingsSection = (page: Page, name: string) =>
  page.getByRole("navigation", { name: "Settings sections" }).getByRole("button", { name, exact: true });

/** A notification the app shows in the corner, found by its text. */
export const toast = (page: Page, text: string) =>
  page.getByRole("region", { name: "Notifications" }).getByRole("status").filter({ hasText: text });

/** Installs a plugin folder as a link, ready for reviews. */
export async function linkPlugin(request: APIRequestContext, source: string, name: string) {
  const installed = await request.post(`${core}/api/v1/plugins/install`, { data: { source, link: true } });
  expect(installed.status(), await installed.text()).toBe(200);
  const row = await installed.json();
  expect(row.name).toBe(name);
  expect(row.usable, JSON.stringify(row)).toBe(true);
}

/** What every plugin folder has besides its manifest, in the fixed layout:
 *  a `view/` folder for its page, and the two schemas, the decision's as given. */
export function layout(dir: string, decisionSchema: unknown = {}) {
  fs.mkdirSync(path.join(dir, "view"), { recursive: true });
  fs.mkdirSync(path.join(dir, "schemas"), { recursive: true });
  fs.writeFileSync(path.join(dir, "schemas", "payload.schema.json"), "{}");
  fs.writeFileSync(path.join(dir, "schemas", "decision.schema.json"), JSON.stringify(decisionSchema));
}

/** Chooses `option` in a submenu of a list page's Filter or Display options
 *  menu, which closes the menu. */
export async function pickIn(page: Page, menu: "Filter" | "Display options", submenu: string, option: string) {
  await page.getByRole("button", { name: menu, exact: true }).click();
  await page.getByRole("menuitem", { name: new RegExp(`^${submenu}`) }).click();
  await page.getByRole("menuitemradio", { name: option, exact: true }).click();
  // the pick closes the menu
  await page.getByRole("menu").first().waitFor({ state: "detached" });
}

/** Turns a filter of a list page's Filter menu on or off, such as Starred. */
export async function toggleFilter(page: Page, label: string) {
  await page.getByRole("button", { name: "Filter", exact: true }).click();
  await page.getByRole("menuitemcheckbox", { name: label }).click();
  await page.getByRole("menu").first().waitFor({ state: "detached" });
}

/** The value a submenu of a list page's Filter or Display options menu shows. */
export async function shownIn(page: Page, menu: "Filter" | "Display options", submenu: string) {
  await page.getByRole("button", { name: menu, exact: true }).click();
  const value = await page
    .getByRole("menuitem", { name: new RegExp(`^${submenu}`) })
    .locator(".menu-value")
    .innerText();
  await page.keyboard.press("Escape");
  return value;
}

/** Replaces speech recognition with a fake, driven by `window.__speak(text)` and `window.__silence()`. */
export async function fakeRecognition(page: Page) {
  await page.addInitScript(() => {
    const w = window as any;
    class FakeRecognition {
      continuous = false;
      interimResults = false;
      lang = "";
      onresult: ((e: unknown) => void) | null = null;
      onerror: ((e: unknown) => void) | null = null;
      onend: (() => void) | null = null;
      start() {
        w.__recognition = this;
      }
      stop() {
        if (w.__recognition === this) w.__recognition = null;
        this.onend?.();
      }
      abort() {
        this.stop();
      }
    }
    w.webkitSpeechRecognition = FakeRecognition;
    w.SpeechRecognition = undefined;
    w.__speak = (text: string) =>
      w.__recognition?.onresult?.({ results: [{ 0: { transcript: text }, isFinal: true }] });
    w.__listening = () => !!w.__recognition;
  });
}

export const speak = (page: Page, text: string) => page.evaluate((t) => (window as any).__speak(t), text);
export const listening = (page: Page) => page.evaluate(() => (window as any).__listening());
