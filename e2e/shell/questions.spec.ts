// Questions an agent asked with `pinrail ask`, answered in the session's
// thread: from the inbox's side panel, on a page, declined, and asked with
// the CLI as an agent does.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { spawn } from "node:child_process";
import os from "node:os";
import path from "node:path";
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
  // one avatar for the set, and each question numbered
  await expect(thread(page).getByTestId("question-set").locator(".msg-avatar")).toHaveCount(1);
  await expect(question(page, "Deploy to staging")).toContainText("1/3");
  // each question's box: with a choice made, a note on it
  const note = question(page, "Deploy to staging").getByRole("textbox", {
    name: "Note on Deploy to staging or production?",
  });
  await note.fill("after the notes");
  // Esc leaves the box, and the keys pick choices again
  await page.keyboard.press("Escape");
  await expect(note).not.toBeFocused();
  // the box at the bottom comments on the whole set
  await page.getByRole("textbox", { name: "Comment to the agent" }).fill("good questions");

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
  expect((await (await page.request.get(`${core}/api/v1/reviews/${id}`)).json()).agent_note).toBe("good questions");
});

test("questions from no session open a thread of their own, answered in the person's own words", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  await clearInbox(page.request);
  const { id } = await send(page.request, asked(null, "Alone: deploy"));
  await page.goto(`/#/reviews/${id}`);
  await expect(thread(page).getByTestId("question")).toHaveCount(3);
  await expect(thread(page).getByTestId("thread-review")).toHaveCount(0);

  // with nothing picked, a question's box answers in the person's own words
  await page
    .getByRole("textbox", { name: "Answer to Deploy to staging or production? in your own words" })
    .fill("neither, wait");
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
  await page.getByRole("button", { name: "Decline" }).click();
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

test("questions stand apart from reviews in the inbox, and their session waits in the sidebar", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  await send(page.request, asked(session, "Apart: deploy"));
  await send(page.request, {
    plugin: "list",
    title: "Apart: the schema",
    origin: { repo: "acme/api" },
    requested_by: "claude-code",
    payload: onePayload,
  });
  await page.goto("/#/");
  await expect(row(page, "Apart: deploy")).toHaveClass(/is-question/);
  await expect(row(page, "Apart: deploy").getByLabel("Asks you")).toBeVisible();
  await expect(row(page, "Apart: deploy")).toContainText("asks you");
  await expect(row(page, "Apart: the schema")).not.toHaveClass(/is-question/);
  await expect(row(page, "Apart: the schema")).toContainText("list");

  const waiting = page.getByRole("region", { name: "Sessions waiting" }).getByTestId("waiting-session");
  await expect(waiting).toHaveCount(1);
  await expect(waiting).toContainText("claude-code");
  await waiting.click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}$`));

  // once nothing waits in it, the session leaves the sidebar
  await clearInbox(page.request);
  await expect(waiting).toHaveCount(0);
});

test("with nothing waiting, the chat box is there but cannot be written in", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  await clearInbox(page.request);
  const { id } = await send(page.request, asked(fresh(), "Idle: deploy"));
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
  await page.goto(`/#/reviews/${id}`);
  await expect(page.getByRole("textbox", { name: "Comment to the agent" })).toBeDisabled();
  await expect(page.getByTestId("send-answers")).toHaveCount(0);
});

test("the question picked keeps its keys, and a key pick moves on to the next", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  await clearInbox(page.request);
  const { id } = await send(page.request, asked(fresh(), "Keys: deploy"));
  await page.goto(`/#/reviews/${id}`);
  const first = question(page, "Deploy to staging");
  const second = question(page, "Keep the old endpoint");
  // a click picks, and the question stays the one the keys answer
  await first.getByRole("radio", { name: "staging" }).click();
  await expect(first.locator(".chip-key")).toHaveText(["1", "2"]);
  await expect(second.locator(".chip-key")).toHaveCount(0);
  // a key picks and moves on, where a yes-no question shows Y and N
  await page.keyboard.press("2");
  await expect(first.getByRole("radio", { name: "production" })).toHaveAttribute("aria-checked", "true");
  await expect(second.locator(".chip-key")).toHaveText(["Y", "N"]);
});

test("an agent asking with pinrail ask gets the answers and the comment written in the app", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  await clearInbox(page.request);
  const exe = process.platform === "win32" ? ".exe" : "";
  const cli = path.join(__dirname, "..", "..", "cli", "target", "debug", `pinrail${exe}`);
  const session = fresh();
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    PINRAIL_URL: core,
    CLAUDECODE: "1",
    CLAUDE_CODE_SESSION_ID: session,
  };
  // the markdown an agent reads, not the JSON a script does
  delete env.PINRAIL_JSON;
  const agent = spawn(
    cli,
    [
      "ask",
      "Deploy to staging or production?",
      "--choice",
      "staging",
      "--choice",
      "production",
      "--ask",
      "Keep the old endpoint?",
      "--yes-no",
    ],
    { cwd: os.tmpdir(), env },
  );
  let stdout = "";
  let stderr = "";
  agent.stdout.on("data", (d) => (stdout += d));
  agent.stderr.on("data", (d) => (stderr += d));
  const exited = new Promise<number | null>((resolve) => agent.on("close", resolve));

  await expect.poll(() => stderr.match(/review (r_[0-9A-Z]+) submitted/)?.[1], { message: stderr }).toBeTruthy();
  // the questions are in the agent's session
  await page.goto(`/#/sessions/claude-code/${session}`);
  await question(page, "Deploy to staging").getByRole("radio", { name: "production" }).click();
  await question(page, "Keep the old endpoint")
    .getByRole("textbox", { name: "Answer to Keep the old endpoint? in your own words" })
    .fill("for one release");
  await page.getByRole("textbox", { name: "Comment to the agent" }).fill("thanks for asking");
  await page.getByTestId("send-answers").click();

  expect(await exited, stderr).toBe(0);
  expect(stdout).toBe(
    "1. Deploy to staging or production?\nproduction\n\n" +
      "2. Keep the old endpoint?\nAnswered in their own words:\nfor one release\n\n" +
      "Comment: thanks for asking\n",
  );
});
