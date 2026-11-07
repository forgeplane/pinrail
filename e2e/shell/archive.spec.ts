// Archiving an agent's session: from its thread's header, from a row of the
// Sessions page and with E, the Show archived filter, and new activity
// bringing an archived session back.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { clearInbox, core, openReviewsIn, toggleFilter } from "./helpers";

let n = 0;
const fresh = () => `s-archive-${Date.now()}-${++n}`;

async function ask(request: APIRequestContext, session: string, title: string) {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: {
      kind: "question",
      title,
      origin: { repo: "acme/api" },
      requested_by: "claude-code",
      session,
      payload: { questions: [{ id: "q1", question: "Ship it?", answer: "yes-no" }] },
    },
  });
  expect(response.status(), await response.text()).toBe(201);
}

const row = (page: Page, session: string) => page.getByTestId("session-row").filter({ hasText: session });

test.afterEach(({ request }) => openReviewsIn(request, "panel"));

test("a session is archived from its thread, and comes back with a new question", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  const other = fresh();
  await ask(page.request, other, "Archive: stays");
  await ask(page.request, session, "Archive: first");
  await page.goto(`/#/sessions/claude-code/${session}`);
  const thread = page.getByTestId("thread");

  await thread.getByRole("button", { name: "Archive" }).click();
  await expect(thread.getByText("Archived", { exact: true })).toBeVisible();
  await expect(thread.getByRole("button", { name: "Unarchive" })).toBeVisible();
  // the Sessions page leaves it out
  await page.goto("/#/sessions");
  await expect(row(page, other)).toHaveCount(1);
  await expect(row(page, session)).toHaveCount(0);

  // the agent asks again, and the session is back
  await ask(page.request, session, "Archive: again");
  await expect(row(page, session)).toHaveCount(1);
  await expect(row(page, session).getByText("Archived", { exact: true })).toHaveCount(0);
});

test("a row is archived with its button and with E, and Show archived brings it back", async ({ page }) => {
  await clearInbox(page.request);
  await openReviewsIn(page.request, "page");
  const first = fresh();
  const second = fresh();
  await ask(page.request, first, "Archive: row one");
  await ask(page.request, second, "Archive: row two");
  await page.goto("/#/sessions");

  // the row's button, without opening the session
  await row(page, first).hover();
  await row(page, first).getByRole("button", { name: "Archive" }).click();
  await expect(row(page, first)).toHaveCount(0);
  await expect(page).toHaveURL(/#\/sessions$/);

  // E on the focused row
  await row(page, second).hover();
  await page.keyboard.press("e");
  await expect(row(page, second)).toHaveCount(0);

  // Show archived lists them with the others, marked, and they come back from there
  await toggleFilter(page, "Show archived");
  await expect(page).toHaveURL(/archived=1/);
  await expect(row(page, first).getByText("Archived", { exact: true })).toBeVisible();
  await expect(row(page, second).getByText("Archived", { exact: true })).toBeVisible();
  await row(page, first).getByRole("button", { name: "Unarchive" }).click();
  await expect(row(page, first).getByText("Archived", { exact: true })).toHaveCount(0);
  await toggleFilter(page, "Show archived");
  await expect(row(page, first)).toHaveCount(1);
  await expect(row(page, second)).toHaveCount(0);
});

test("E archives the session open in the side panel", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  await ask(page.request, session, "Archive: in the panel");
  await page.goto("/#/sessions");
  await row(page, session).click();
  const panel = page.getByTestId("session-panel");
  await expect(panel.getByRole("button", { name: "Archive" })).toBeVisible();
  // the focus off the answer field, as after reading the thread
  await panel.getByRole("heading", { level: 1 }).click();
  await page.keyboard.press("e");
  await expect(panel.getByRole("button", { name: "Unarchive" })).toBeVisible();
  await expect(row(page, session)).toHaveCount(0);
});
