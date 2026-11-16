// The line that says a session's agent is working: above the message box,
// on a line whose height is kept, for as long as the agent works.

import { expect, test, type APIRequestContext, type Page } from "../fixtures";
import { core } from "../helpers";

let n = 0;
const fresh = () => `s-working-${Date.now()}-${++n}`;

async function post(request: APIRequestContext, session: string, path: string, data: unknown) {
  const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/${path}`, { data });
  expect(response.status(), await response.text()).toBeLessThan(300);
}

const working = (page: Page) => page.getByTestId("thread").getByRole("status", { name: "Claude Code is working" });

/** Where the message box is, and whether the working line sits right above
 *  it, under the thread. */
const placed = (page: Page) =>
  page.locator(".thread-main").evaluate((main) => {
    const box = main.querySelector(".thread-composer")!.getBoundingClientRect();
    const line = main.querySelector("[data-testid='turn-working']")?.getBoundingClientRect();
    return {
      box: Math.round(box.top),
      above: !!line && line.bottom <= box.top && box.top - line.bottom < 24,
      inFooter: !!main.querySelector(".thread-footer [data-testid='turn-working']"),
    };
  });

test("the working line shows above the message box as the agent reports", async ({ page }) => {
  const session = fresh();
  await post(page.request, session, "status", { kind: "start", text: "Fix the login", origin: { repo: "acme/api" } });
  await post(page.request, session, "reports", { event: "prompt", detail: { prompt: "Start the next item" } });
  await post(page.request, session, "status", { kind: "start", text: "Build the parser" });
  await post(page.request, session, "status", { kind: "note", text: "Read the grammar first." });
  await page.goto(`/#/sessions/claude-code/${session}`);

  await expect(page.getByTestId("thread")).toContainText("Read the grammar first.");
  await expect(working(page)).toHaveCount(1);
  await expect.poll(async () => (await placed(page)).above).toBe(true);
  expect((await placed(page)).inFooter).toBe(true);
});

test("the working line shows while the agent works between turns", async ({ page }) => {
  const session = fresh();
  await post(page.request, session, "status", { kind: "start", text: "Fix the login", origin: { repo: "acme/api" } });
  await post(page.request, session, "reports", { event: "prompt", detail: { prompt: "Run the tests" } });
  await post(page.request, session, "reports", { event: "turn_ended", end: "done", detail: { reply: "They pass." } });
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(page.getByTestId("turn-reply")).toContainText("They pass.");
  await expect(working(page)).toHaveCount(0);
  const idle = (await placed(page)).box;

  // the agent goes on by itself, with no new prompt; the message box stays
  // where it was
  await post(page.request, session, "reports", { event: "working" });
  await expect(working(page)).toHaveCount(1);
  expect(await placed(page)).toMatchObject({ box: idle, above: true });

  // and stops
  await post(page.request, session, "reports", { event: "turn_ended", end: "done", detail: {} });
  await expect(working(page)).toHaveCount(0);
  expect((await placed(page)).box).toBe(idle);
});
