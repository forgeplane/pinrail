// A link the agent writes in a session's thread opens in the browser.

import { expect, test, type Page } from "@playwright/test";
import { core } from "./helpers";

let n = 0;
const fresh = () => `s-links-${Date.now()}-${++n}`;

async function report(page: Page, session: string, data: Record<string, unknown>) {
  const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, { data });
  expect(response.status(), await response.text()).toBe(200);
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    (window as unknown as { opened: string[] }).opened = [];
    window.open = ((url: string) => {
      (window as unknown as { opened: string[] }).opened.push(url);
      return null;
    }) as typeof window.open;
  });
});

const opened = (page: Page) => page.evaluate(() => (window as unknown as { opened: string[] }).opened);

test("a link in a status opens in the browser", async ({ page }) => {
  const session = fresh();
  await report(page, session, { text: "Plan at https://example.com/plan", kind: "start" });
  await report(page, session, { text: "Reading https://example.com/step and *.md files" });
  await report(page, session, { text: "Opened https://example.com/pr/214", kind: "done" });
  await page.goto(`/#/sessions/claude-code/${session}`);

  const thread = page.getByTestId("thread");
  for (const [where, url] of [
    ["started", "https://example.com/plan"],
    ["status", "https://example.com/step"],
    ["finished", "https://example.com/pr/214"],
  ]) {
    await thread.getByTestId(where).getByRole("link", { name: url }).click();
    await expect.poll(async () => (await opened(page)).at(-1)).toBe(url);
  }
  // the rest of a status stays as the agent wrote it
  await expect(thread.getByTestId("status")).toContainText("and *.md files");
  await expect(page).toHaveURL(new RegExp(session));
});

test("a status shows markup as text, and only a web link as a link", async ({ page }) => {
  const session = fresh();
  await report(page, session, { text: "Ran <b>tests</b> javascript:alert(1)" });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const status = page.getByTestId("thread").getByTestId("status");
  await expect(status).toContainText("Ran <b>tests</b> javascript:alert(1)");
  await expect(status.getByRole("link")).toHaveCount(0);
});

test("a link to this machine in the agent's text opens too", async ({ page }) => {
  const session = fresh();
  const url = "http://127.0.0.1:4321/docs/";
  await report(page, session, { text: `Serving at ${url}` });
  await report(page, session, { text: `The preview is at ${url}`, kind: "note" });
  await page.goto(`/#/sessions/claude-code/${session}`);

  const thread = page.getByTestId("thread");
  await thread.getByTestId("status").getByRole("link", { name: url }).click();
  await expect.poll(async () => (await opened(page)).length).toBe(1);
  await thread.locator(".thread-note-body").getByRole("link", { name: url }).click();
  await expect.poll(() => opened(page)).toEqual([url, url]);
});
