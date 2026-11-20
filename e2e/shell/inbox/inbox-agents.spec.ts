// The inbox's Agents section: the sessions whose agent waits on the person
// in its terminal, or finished a turn the person has not seen.

import { expect, test, type APIRequestContext, type Page } from "../fixtures";
import { clearInbox, core, sessionId } from "../helpers";

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

const agentRow = (page: Page, title: string) => page.getByTestId("agent-row").filter({ hasText: title });

test("the Agents section lists what waits in a terminal and unseen finished turns, opened with Enter and seen with E", async ({
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
  const section = page.getByRole("region", { name: "Agents" });
  await expect(section).toBeVisible();
  await expect(agentRow(page, `Agents: deploy ${run}`)).toContainText("Waiting for permission: Bash");
  await expect(agentRow(page, `Agents: lint ${run}`)).toContainText(/Finished \d+s ago/);

  // E marks the finished row seen, which takes it off the list
  await agentRow(page, `Agents: lint ${run}`).hover();
  await page.keyboard.press("e");
  await expect(agentRow(page, `Agents: lint ${run}`)).toHaveCount(0);

  // the row lit opens its session with Enter
  await agentRow(page, `Agents: deploy ${run}`).hover();
  await expect(agentRow(page, `Agents: deploy ${run}`)).toHaveClass(/is-focused/);
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(
    new RegExp(`#/sessions/claude-code/${await sessionId(page.request, "claude-code", waiting)}$`),
  );
  await expect(page.getByTestId("thread").getByTestId("presence-chip")).toHaveText("Needs you · Bash permission");
});
