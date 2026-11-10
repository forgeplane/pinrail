// A session's thread opens at its newest message, and stays there while
// what it shows finishes laying out.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core } from "./helpers";

let n = 0;
const fresh = () => `s-scroll-${Date.now()}-${++n}`;

async function report(request: APIRequestContext, session: string, text: string, kind?: string) {
  const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: kind ? { text, kind } : { text },
  });
  expect(response.status(), await response.text()).toBe(200);
}

/** How far the timeline is from its end, in pixels. */
const fromEnd = (page: Page) =>
  page.locator(".thread-items").evaluate((el) => Math.round(el.scrollHeight - el.clientHeight - el.scrollTop));

test("a session opens at its newest message, under a long waiting question", async ({ page, request }) => {
  const session = fresh();
  await report(request, session, "Fix the scrolling", "start");
  for (let i = 0; i < 12; i++) await report(request, session, `Step ${i + 1} of the work`);
  const diff = ["```diff", "--- a/x.ts", "+++ b/x.ts", "@@ -1,40 +1,40 @@"]
    .concat(Array.from({ length: 40 }, (_, i) => `-old line ${i}\n+new line ${i}`))
    .concat("```")
    .join("\n");
  const asked = await request.post(`${core}/api/v1/reviews`, {
    data: {
      kind: "question",
      title: "Commit it?",
      requested_by: "claude-code",
      session,
      payload: { questions: [{ id: "q1", question: "Commit it?", answer: "yes-no", context: diff }] },
    },
  });
  expect(asked.status(), await asked.text()).toBe(201);

  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(page.getByText("Commit it?").first()).toBeVisible();
  // once everything has laid out, the end still shows
  await page.waitForTimeout(1000);
  expect(await fromEnd(page)).toBeLessThan(2);
});

test("the newest message stays in view as the agent reports, until the person scrolls up", async ({
  page,
  request,
}) => {
  const session = fresh();
  await report(request, session, "Fix the scrolling", "start");
  for (let i = 0; i < 20; i++) await report(request, session, `Step ${i + 1} of the work`);
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(page.getByText("Step 20 of the work")).toBeVisible();

  await report(request, session, "Step 21 of the work");
  await expect(page.getByText("Step 21 of the work")).toBeVisible();
  await expect.poll(() => fromEnd(page)).toBeLessThan(2);

  // scrolled up, the thread stays where the person put it
  await page.locator(".thread-items").evaluate((el) => (el.scrollTop = 0));
  await report(request, session, "Step 22 of the work");
  await expect(page.getByText("Step 22 of the work")).toBeAttached();
  await page.waitForTimeout(300);
  expect(await page.locator(".thread-items").evaluate((el) => el.scrollTop)).toBe(0);
});
