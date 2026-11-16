// The line that says a session's agent is working: at the end of the
// thread, for as long as the agent works.

import { expect, test, type APIRequestContext, type Page } from "../fixtures";
import { core } from "../helpers";

let n = 0;
const fresh = () => `s-working-${Date.now()}-${++n}`;

async function post(request: APIRequestContext, session: string, path: string, data: unknown) {
  const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/${path}`, { data });
  expect(response.status(), await response.text()).toBeLessThan(300);
}

const working = (page: Page) => page.getByTestId("thread").getByRole("status", { name: "Claude Code is working" });

/** Whether the working line is the last thing in the thread. */
const isLast = (page: Page) =>
  page.locator(".thread-items").evaluate((items) => {
    const line = items.querySelector("[data-testid='turn-working']");
    if (!line) return false;
    const bottom = line.getBoundingClientRect().bottom;
    return [...items.querySelectorAll("*")].every((el) => el.getBoundingClientRect().bottom <= bottom + 1);
  });

test("the working line stays at the end of the thread as the agent reports", async ({ page }) => {
  const session = fresh();
  await post(page.request, session, "status", { kind: "start", text: "Fix the login", origin: { repo: "acme/api" } });
  await post(page.request, session, "reports", { event: "prompt", detail: { prompt: "Start the next item" } });
  await post(page.request, session, "status", { kind: "start", text: "Build the parser" });
  await post(page.request, session, "status", { kind: "note", text: "Read the grammar first." });
  await page.goto(`/#/sessions/claude-code/${session}`);

  await expect(page.getByTestId("thread")).toContainText("Read the grammar first.");
  await expect(working(page)).toHaveCount(1);
  await expect.poll(() => isLast(page)).toBe(true);
});

test("the working line shows while the agent works between turns", async ({ page }) => {
  const session = fresh();
  await post(page.request, session, "status", { kind: "start", text: "Fix the login", origin: { repo: "acme/api" } });
  await post(page.request, session, "reports", { event: "prompt", detail: { prompt: "Run the tests" } });
  await post(page.request, session, "reports", { event: "turn_ended", end: "done", detail: { reply: "They pass." } });
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(page.getByTestId("turn-reply")).toContainText("They pass.");
  await expect(working(page)).toHaveCount(0);

  // the agent goes on by itself, with no new prompt
  await post(page.request, session, "reports", { event: "working" });
  await expect(working(page)).toHaveCount(1);
  await expect.poll(() => isLast(page)).toBe(true);

  // and stops
  await post(page.request, session, "reports", { event: "turn_ended", end: "done", detail: {} });
  await expect(working(page)).toHaveCount(0);
});
