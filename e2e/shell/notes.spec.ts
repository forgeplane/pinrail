// Notes an agent sends with `pinrail status --note`: Markdown shown as a card
// in the session's thread, which informs and changes nothing else.

import { expect, test, type Page } from "@playwright/test";
import { core } from "./helpers";

let n = 0;
/** A session of its own for each test, so threads never share statuses. */
const fresh = () => `s-note-${Date.now()}-${++n}`;

async function report(page: Page, session: string, body: Record<string, unknown>) {
  const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, { data: body });
  expect(response.status(), await response.text()).toBe(204);
}

test("a note shows in the thread as Markdown, and a status after it as a status", async ({ page }) => {
  const session = fresh();
  await report(page, session, { text: "Add rate limiting", kind: "start" });
  await report(page, session, { text: "Running the tests" });
  await report(page, session, {
    text: "## What I found\n\n- **3** callers of the old endpoint\n- all in `billing/`\n\nSee [the docs](https://example.com/limits).",
    kind: "note",
  });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const note = page.getByTestId("note");
  await expect(note.getByRole("heading", { name: "What I found" })).toBeVisible();
  await expect(note.getByRole("listitem")).toHaveText(["3 callers of the old endpoint", "all in billing/"]);
  await expect(note.locator("strong")).toHaveText("3");
  await expect(note.getByRole("link", { name: "the docs" })).toHaveAttribute("href", "https://example.com/limits");
  // the step before it is a status of its own
  await expect(page.getByTestId("status")).toHaveText([/Running the tests/]);

  // a note sent while the thread is open shows at once
  await report(page, session, { text: "Pushed the branch.", kind: "note" });
  await expect(page.getByTestId("note")).toHaveCount(2);
  await expect(page.getByTestId("note").last()).toContainText("Pushed the branch.");
});

test("a long note folds behind Show more", async ({ page }) => {
  const session = fresh();
  const lines = Array.from({ length: 40 }, (_, i) => `- line ${i + 1}`).join("\n");
  await report(page, session, { text: lines, kind: "note" });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const body = page.getByTestId("note").locator(".thread-note-body");
  const folded = (await body.boundingBox())?.height ?? 0;
  expect(folded).toBeLessThan(300);
  await page.getByTestId("note").getByRole("button", { name: "Show more" }).click();
  expect((await body.boundingBox())?.height ?? 0).toBeGreaterThan(folded * 2);
  await page.getByTestId("note").getByRole("button", { name: "Show less" }).click();
  expect((await body.boundingBox())?.height ?? 0).toBeLessThan(300);
});

test("a note is not the latest status on the Sessions page", async ({ page }) => {
  const session = fresh();
  await report(page, session, { text: "Running the tests" });
  await report(page, session, { text: "A summary nobody should see as the status", kind: "note" });
  await page.goto(`/#/sessions?q=${session}`);
  const row = page.getByTestId("session-row").filter({ hasText: session });
  await expect(row.getByTestId("session-status")).toHaveText(/Running the tests/);
});
