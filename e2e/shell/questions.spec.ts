// Questions an agent asked with `pinrail ask`, answered in the session's
// thread: from the inbox's side panel, on a page, and declined.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { clearInbox, core, openReviewsIn, onePayload } from "./helpers";

const row = (page: Page, title: string) => page.getByTestId("review-row").filter({ hasText: title });
const thread = (page: Page) => page.getByTestId("thread");
const question = (page: Page, text: string) => page.getByTestId("question").filter({ hasText: text });

let n = 0;
/** A session of its own for each test, so threads never share reviews. */
const fresh = () => `s-spec-${Date.now()}-${++n}`;

async function send(request: APIRequestContext, body: Record<string, unknown>) {
  const response = await request.post(`${core}/api/v1/reviews`, { data: body });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { id: string };
}

const asked = (session: string | null, title: string) => ({
  kind: "question",
  title,
  origin: { repo: "acme/api", ref: "main" },
  requested_by: "claude-code",
  session,
  payload: {
    questions: [
      {
        id: "q1",
        question: "Deploy to staging or production?",
        answer: "choice",
        choices: ["staging", "production"],
        recommended: "staging",
        context: "The release notes **PR** is still open.",
      },
      { id: "q2", question: "Keep the old endpoint?", answer: "yes-no" },
      { id: "q3", question: "What should it be called?", answer: "text" },
    ],
  },
});

async function decisionOf(request: APIRequestContext, id: string) {
  const review = await (await request.get(`${core}/api/v1/reviews/${id}`)).json();
  return review as { status: string; decision: { data: unknown } | null; discarded_reason: string | null };
}

test.afterEach(({ request }) => openReviewsIn(request, "panel"));

test("questions open in their session's thread in the side panel, and are answered there", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  await send(page.request, {
    plugin: "list",
    title: "Thread: the schema",
    origin: { repo: "acme/api" },
    requested_by: "claude-code",
    session,
    payload: onePayload,
  });
  const { id } = await send(page.request, asked(session, "Thread: deploy"));
  await page.goto("/#/");
  await row(page, "Thread: deploy").click();

  // the session's review before the questions, and the questions' context as Markdown
  await expect(thread(page).getByTestId("thread-review")).toContainText("Thread: the schema");
  await expect(question(page, "Deploy to staging").locator("strong")).toHaveText("PR");

  // a pick moves on: 2 picks production, then y answers the yes-no question
  await page.keyboard.press("2");
  await expect(question(page, "Deploy to staging").getByRole("radio", { name: "production" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await page.keyboard.press("y");
  await expect(question(page, "Keep the old endpoint").getByRole("radio", { name: "Yes" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await question(page, "Deploy to staging").getByRole("button", { name: "Add a note" }).click();
  await page.getByRole("textbox", { name: "Note on Deploy to staging or production?" }).fill("after the notes");

  const sendButton = page.getByTestId("send-answers");
  await expect(sendButton).toHaveText(/Send, 1 unanswered/);
  await sendButton.click();
  await expect.poll(async () => (await decisionOf(page.request, id)).status).toBe("decided");
  expect((await decisionOf(page.request, id)).decision?.data).toEqual({
    answers: [
      { id: "q1", choice: "production", note: "after the notes" },
      { id: "q2", yes: true },
      { id: "q3", unanswered: true },
    ],
  });
});

test("questions from no session open a thread of their own, answered in the person's own words", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  await clearInbox(page.request);
  const { id } = await send(page.request, asked(null, "Alone: deploy"));
  await page.goto(`/#/reviews/${id}`);
  await expect(thread(page).getByTestId("question")).toHaveCount(3);
  await expect(thread(page).getByTestId("thread-review")).toHaveCount(0);

  await question(page, "Deploy to staging").getByRole("button", { name: "Answer in your own words" }).click();
  await page.getByRole("textbox", { name: "Answer to Deploy to staging or production?" }).fill("neither, wait");
  await page.getByRole("textbox", { name: "Answer to What should it be called?" }).fill("v2");
  await page.keyboard.press(process.platform === "darwin" ? "Meta+Enter" : "Control+Enter");
  await expect.poll(async () => (await decisionOf(page.request, id)).status).toBe("decided");
  expect((await decisionOf(page.request, id)).decision?.data).toEqual({
    answers: [
      { id: "q1", text: "neither, wait" },
      { id: "q2", unanswered: true },
      { id: "q3", text: "v2" },
    ],
  });

  // the answers then show as the person's messages
  await page.goto(`/#/reviews/${id}`);
  await expect(thread(page).getByTestId("answer")).toHaveCount(3);
  await expect(thread(page).getByTestId("answer").first()).toContainText("neither, wait");
});

test("declining tells the agent to stop, with the reason", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  await clearInbox(page.request);
  const { id } = await send(page.request, asked(fresh(), "Decline: deploy"));
  await page.goto(`/#/reviews/${id}`);
  await thread(page).getByRole("button", { name: "Decline" }).click();
  const dialog = page.getByRole("dialog", { name: "Decline to answer?" });
  await dialog.getByRole("textbox", { name: "Reason" }).fill("ask in the PR");
  await dialog.getByRole("button", { name: "Decline" }).click();
  await expect.poll(async () => (await decisionOf(page.request, id)).status).toBe("discarded");
  expect((await decisionOf(page.request, id)).discarded_reason).toBe("ask in the PR");
});

test("the Sessions page lists the sessions, those waiting first, and opens a session's thread", async ({ page }) => {
  await clearInbox(page.request);
  const answered = fresh();
  const waiting = fresh();
  const { id } = await send(page.request, asked(answered, "Sessions: answered"));
  const done = await page.request.post(`${core}/api/v1/reviews/${id}/decision`, {
    data: {
      data: {
        answers: [
          { id: "q1", choice: "staging" },
          { id: "q2", yes: true },
          { id: "q3", text: "v2" },
        ],
      },
    },
  });
  expect(done.status()).toBe(200);
  await send(page.request, { ...asked(waiting, "Sessions: waiting"), requested_by: "codex" });

  await page.goto("/#/");
  await page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: "Sessions" }).click();
  const rows = page.getByTestId("session-row");
  await expect(rows.first()).toContainText(waiting);
  await expect(rows.first()).toContainText("1 question");
  await expect(rows.filter({ hasText: answered })).toContainText("1 question");

  await page.getByRole("searchbox", { name: "Search sessions" }).fill(answered);
  await expect(rows).toHaveCount(1);
  await rows.first().click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${answered}$`));
  await expect(thread(page).getByTestId("answer")).toHaveCount(3);
});

test("a question's thread links to its session's page", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  await clearInbox(page.request);
  const session = fresh();
  const { id } = await send(page.request, asked(session, "Link: deploy"));
  await page.goto(`/#/reviews/${id}`);
  await thread(page)
    .getByRole("link", { name: /^session / })
    .click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}$`));
  await expect(thread(page).getByTestId("question")).toHaveCount(3);
});
