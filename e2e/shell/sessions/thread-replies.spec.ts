// An agent's reply in a session's thread: folded to a few lines, with
// Show more, only while it does not fit.

import { expect, test, type APIRequestContext } from "../fixtures";
import { core } from "../helpers";

let n = 0;
const fresh = () => `s-replies-${Date.now()}-${++n}`;

async function post(request: APIRequestContext, session: string, path: string, data: unknown) {
  const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/${path}`, { data });
  expect(response.status(), await response.text()).toBeLessThan(300);
}

/** A session with one turn that ended with `reply`. */
async function replied(request: APIRequestContext, session: string, reply: string) {
  await post(request, session, "status", { kind: "start", text: "Check the reviews", origin: { repo: "acme/api" } });
  await post(request, session, "reports", { event: "prompt", detail: { prompt: "Why did it fail?" } });
  await post(request, session, "reports", { event: "turn_ended", end: "done", detail: { reply } });
}

const SHORT =
  "That failure is expected. Round 2 was withdrawn when I sent round 3 to replace it. " +
  "Round 3 of `PIN-400` is still waiting for your review in Pinrail.";

test("a reply that fits once the window is wider has no Show more", async ({ page }) => {
  const session = fresh();
  await replied(page.request, session, SHORT);
  await page.setViewportSize({ width: 800, height: 900 });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const thread = page.getByTestId("thread");
  await expect(thread.getByRole("button", { name: "Show more" })).toBeVisible();

  await page.setViewportSize({ width: 1500, height: 900 });
  await expect(thread.getByRole("button", { name: "Show more" })).toHaveCount(0);
  await expect(thread.locator(".thread-turn-folded")).not.toHaveClass(/is-folded/);
});

test("a long reply keeps Show less while open, whatever the width", async ({ page }) => {
  const session = fresh();
  await replied(page.request, session, Array.from({ length: 12 }, (_, i) => `Line ${i + 1}.`).join("\n\n"));
  await page.goto(`/#/sessions/claude-code/${session}`);
  const thread = page.getByTestId("thread");
  await thread.getByRole("button", { name: "Show more" }).click();
  await page.setViewportSize({ width: 1400, height: 900 });
  await expect(thread.getByRole("button", { name: "Show less" })).toBeVisible();
  await thread.getByRole("button", { name: "Show less" }).click();
  await expect(thread.getByRole("button", { name: "Show more" })).toBeVisible();
});
