// A session whose turn finished shows Finished until the person has looked
// at it, and Mark as unread brings Finished back.

import { expect, test, type APIRequestContext } from "./fixtures";
import { core } from "./helpers";

let n = 0;
const fresh = () => `s-seen-${Date.now()}-${++n}`;

async function post(request: APIRequestContext, session: string, path: string, data: unknown) {
  const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/${path}`, { data });
  expect(response.status(), await response.text()).toBeLessThan(300);
}

/** A session the agent named, whose hooks report one turn that ended. */
async function finishedTurn(request: APIRequestContext, session: string) {
  await post(request, session, "status", {
    kind: "start",
    text: "Fix the login redirect",
    origin: { repo: "acme/api" },
  });
  await post(request, session, "reports", { event: "prompt", detail: { prompt: "Fix it" } });
  await post(request, session, "reports", { event: "turn_ended", end: "done", detail: { reply: "Fixed." } });
}

test("a finished session is seen once its thread was shown, and Mark as unread brings Finished back", async ({
  page,
}) => {
  const session = fresh();
  await finishedTurn(page.request, session);
  const mark = page.locator(`[data-testid="sidebar-session"][href$="/${session}"]`);

  await page.goto("/#/");
  await expect(mark).toHaveAttribute("data-state", "finished");

  await page.goto(`/#/sessions/claude-code/${session}`);
  const chip = page.getByTestId("thread").getByTestId("presence-chip");
  await expect(chip).toHaveText("Finished");
  // shown for a second with the window focused, it is seen
  await expect(chip).toHaveText("Idle", { timeout: 5_000 });
  await expect(mark).toHaveAttribute("data-state", "idle");

  await page.getByTestId("thread").getByRole("button", { name: "Mark as unread" }).click();
  await expect(chip).toHaveText("Finished");
  await expect(mark).toHaveAttribute("data-state", "finished");
  // still in view, it stays unread until the person comes back to it
  await page.waitForTimeout(2_000);
  await expect(chip).toHaveText("Finished");
  await expect(page.getByTestId("thread").getByRole("button", { name: "Mark as unread" })).toHaveCount(0);

  // shown again later, it is seen again
  await page.goto("/#/");
  await expect(mark).toHaveAttribute("data-state", "finished");
  await mark.click();
  await expect(chip).toHaveText("Idle", { timeout: 5_000 });
});
