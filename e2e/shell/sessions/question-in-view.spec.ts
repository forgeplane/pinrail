// A question opened from the inbox shows in view in its session's thread in
// the side panel, wherever it is in the thread. It runs in WebKit too, the
// app's engine.

import { expect, test, type APIRequestContext } from "../fixtures";
import { clearInbox, core } from "../helpers";

let n = 0;
const fresh = () => `s-in-view-${Date.now()}-${++n}`;

async function ask(request: APIRequestContext, session: string, title: string) {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: {
      kind: "question",
      title,
      origin: { repo: "acme/api", ref: "main" },
      requested_by: "claude-code",
      session,
      payload: { questions: [{ id: "q1", question: "Deploy to staging or production?", answer: "yes-no" }] },
    },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { id: string };
}

test("a question opened from the inbox is in view, with much of the session after it", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  const note = (text: string) =>
    page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
      data: { kind: "note", text, origin: { repo: "acme/api" } },
    });
  for (let i = 1; i <= 30; i++) await note(`Note ${i} before the questions.`);
  const { id } = await ask(page.request, session, "Later: deploy");
  // the agent goes on working after it asked
  for (let i = 1; i <= 60; i++) await note(`Note ${i} after the questions.`);
  await page.goto("/#/");
  await page.getByTestId("review-row").filter({ hasText: "Later: deploy" }).click();
  const set = page.locator(`#set-${id}`);
  await expect(set).toBeVisible();
  await expect(set).toBeInViewport();

  // the agent still at work: what it reports next leaves the question in view
  for (let i = 1; i <= 5; i++) {
    await note(`Still working ${i}.`);
    await page.waitForTimeout(300);
  }
  await page.waitForTimeout(1000);
  await expect(set).toBeInViewport();
});
