// Agents in the inbox: one that waits on the person in its terminal is in
// Waiting on you, and one that finished a turn is an update.

import { expect, test, type APIRequestContext, type Page } from "../fixtures";
import { clearInbox, core } from "../helpers";

let n = 0;
const fresh = () => `s-agents-${Date.now()}-${++n}`;

async function post(request: APIRequestContext, session: string, path: string, data: unknown) {
  const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/${path}`, { data });
  expect(response.status(), await response.text()).toBeLessThan(300);
}

/** A session the agent named, with one turn under way. */
async function started(request: APIRequestContext, session: string, title: string) {
  await post(request, session, "status", { kind: "start", text: title, origin: { repo: "acme/api" } });
  await post(request, session, "reports", { event: "prompt", detail: { prompt: "Go" } });
}

const row = (page: Page, section: string, title: string) =>
  page.getByRole("region", { name: section }).getByTestId("update-row").filter({ hasText: title });

test("an agent waiting in its terminal waits on you, a finished one is an update, E marks it done and Enter opens it", async ({
  page,
}) => {
  await clearInbox(page.request);
  const waiting = fresh();
  const finished = fresh();
  const run = Date.now();
  await started(page.request, waiting, `Agents: deploy ${run}`);
  await post(page.request, waiting, "reports", {
    event: "needs_input",
    wait: "permission",
    detail: { tool: "Bash", summary: "npm run deploy" },
  });
  await started(page.request, finished, `Agents: lint ${run}`);
  await post(page.request, finished, "reports", { event: "turn_ended", end: "done", detail: { reply: "Clean." } });

  await page.goto("/#/");
  const asks = row(page, "Waiting on you", `Agents: deploy ${run}`);
  await expect(asks).toContainText("permission");
  await expect(asks).toContainText("npm run deploy");
  const done = row(page, "Updates", `Agents: lint ${run}`);
  await expect(done).toContainText("finished");

  // E marks the finished row done, which takes it off the list
  await done.hover();
  await page.keyboard.press("e");
  await expect(row(page, "Updates", `Agents: lint ${run}`)).toHaveCount(0);

  // the row lit opens its session with Enter
  await asks.hover();
  await expect(asks).toHaveClass(/is-focused/);
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("thread").getByTestId("presence-chip")).toHaveText("Needs you · Bash permission");
});
