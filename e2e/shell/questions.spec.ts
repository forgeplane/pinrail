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

  // a pick moves on: 2 picks production, then 1 answers yes to the yes-no question
  await page.keyboard.press("2");
  await expect(question(page, "Deploy to staging").getByRole("radio", { name: "production" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await page.keyboard.press("1");
  await expect(question(page, "Keep the old endpoint").getByRole("radio", { name: "Yes" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  // one avatar for the set, and each question numbered
  await expect(thread(page).getByTestId("question-set").locator(".msg-avatar")).toHaveCount(1);
  await expect(question(page, "Deploy to staging")).toContainText("1/3");
  // a note on an answer, written on demand and saved with Enter
  await expect(question(page, "Deploy to staging").getByRole("textbox")).toHaveCount(0);
  await page.getByRole("button", { name: "Add a note to Deploy to staging or production?" }).click();
  const note = page.getByRole("textbox", { name: "Note on Deploy to staging or production?" });
  await expect(note).toBeFocused();
  await note.fill("after the notes");
  await note.press("Enter");
  await expect(note).toHaveCount(0);
  await expect(question(page, "Deploy to staging").getByTestId("saved-note")).toContainText("after the notes");
  // the box at the bottom comments on the whole set
  await page.getByRole("textbox", { name: "Comment to the agent" }).fill("good questions");

  const sendButton = page.getByTestId("send-answers");
  await expect(sendButton).toHaveAccessibleName("Send, 1 unanswered");
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

  // Other… answers in the person's own words
  await question(page, "Deploy to staging").getByRole("button", { name: "Other…" }).click();
  const own = page.getByRole("textbox", { name: "Answer to Deploy to staging or production? in your own words" });
  await own.fill("neither, wait");
  await own.press("Enter");
  await expect(question(page, "Deploy to staging").getByTestId("saved-answer")).toContainText("neither, wait");
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

test("answering in the side panel stays on the thread", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  const { id } = await send(page.request, asked(session, "Stay: deploy"));
  await send(page.request, {
    plugin: "list",
    title: "Stay: the next review",
    origin: { repo: "acme/api" },
    requested_by: "codex",
    payload: onePayload,
  });
  await page.goto("/#/");
  await row(page, "Stay: deploy").click();
  await page.getByRole("radio", { name: "staging" }).click();
  await page.getByTestId("send-answers").click();
  await expect.poll(async () => (await decisionOf(page.request, id)).status).toBe("decided");
  await expect(thread(page).getByTestId("answer")).toHaveCount(3);
  await expect(page).toHaveURL(new RegExp(id));
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

test("a session's thread opens scrolled to its newest message", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  for (const i of [1, 2, 3, 4]) await send(page.request, asked(session, `Bottom: set ${i}`));
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(thread(page).getByTestId("question")).toHaveCount(12);
  const items = thread(page).locator(".thread-items");
  await expect.poll(() => items.evaluate((el) => el.scrollHeight - el.clientHeight - el.scrollTop)).toBeLessThan(2);
  expect(await items.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
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
  // a key picks and moves on, and a yes-no question takes 1 and 2 too
  await page.keyboard.press("2");
  await expect(first.getByRole("radio", { name: "production" })).toHaveAttribute("aria-checked", "true");
  await expect(second.locator(".chip-key")).toHaveText(["1", "2"]);
  // P and N go to the question before and after
  await page.keyboard.press("p");
  await expect(first.locator(".chip-key")).toHaveText(["1", "2"]);
  await page.keyboard.press("n");
  await expect(second.locator(".chip-key")).toHaveText(["1", "2"]);
  // C opens a note on the question
  await page.keyboard.press("c");
  await expect(page.getByRole("textbox", { name: "Note on Keep the old endpoint?" })).toBeFocused();
  await page.keyboard.press("Escape");
  // a second click on the pick unpicks it
  await first.getByRole("radio", { name: "production" }).click();
  await expect(first.getByRole("radio", { name: "production" })).toHaveAttribute("aria-checked", "false");
  await expect(page.getByTestId("send-answers")).toHaveAccessibleName("Send, 3 unanswered");
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
  await question(page, "Keep the old endpoint").getByRole("button", { name: "Other…" }).click();
  await page
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

test("a question that takes several choices says so", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  await clearInbox(page.request);
  const { id } = await send(page.request, {
    ...asked(fresh(), "Several: checks"),
    payload: {
      questions: [
        { id: "q1", question: "Which checks?", answer: "choices", choices: ["lint", "tests"] },
        { id: "q2", question: "Which one?", answer: "choice", choices: ["a", "b"] },
      ],
    },
  });
  await page.goto(`/#/reviews/${id}`);
  await expect(question(page, "Which checks?")).toContainText("Select all that apply");
  await expect(question(page, "Which one?")).not.toContainText("Select all that apply");
});

test("Enter saves an answer in the person's own words, and Shift+Enter starts a new line", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  await clearInbox(page.request);
  const { id } = await send(page.request, asked(fresh(), "Enter: deploy"));
  await page.goto(`/#/reviews/${id}`);
  const first = question(page, "Deploy to staging");
  await first.getByRole("button", { name: "Other…" }).click();
  const own = page.getByRole("textbox", { name: "Answer to Deploy to staging or production? in your own words" });
  await own.fill("neither");
  await own.press("Shift+Enter");
  await own.pressSequentially("wait for Monday");
  await expect(own).toHaveValue("neither\nwait for Monday");
  await own.press("Enter");
  await expect(first.getByTestId("saved-answer")).toContainText("neither\nwait for Monday");
  // the pencil edits it, and the bin deletes it
  await first.getByRole("button", { name: "Edit the answer" }).click();
  await expect(own).toHaveValue("neither\nwait for Monday");
  await own.press("Enter");
  await first.getByTestId("saved-answer").hover();
  await first.getByRole("button", { name: "Delete the answer" }).click();
  await expect(first.getByTestId("saved-answer")).toHaveCount(0);
  // Enter in the last text answer goes on to the comment on them all
  await page.getByRole("textbox", { name: "Answer to What should it be called?" }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("textbox", { name: "Comment to the agent" })).toBeFocused();
});

test("Enter in the comment box sends the answers, and Shift+Enter starts a new line", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  await clearInbox(page.request);
  const { id } = await send(page.request, asked(fresh(), "Comment: deploy"));
  await page.goto(`/#/reviews/${id}`);
  await page.getByRole("radio", { name: "staging" }).click();
  const comment = page.getByRole("textbox", { name: "Comment to the agent" });
  await comment.fill("go ahead");
  await comment.press("Shift+Enter");
  await comment.pressSequentially("after lunch");
  await expect(page.getByText("2 unanswered · Enter to send")).toBeVisible();
  await comment.press("Enter");
  await expect.poll(async () => (await decisionOf(page.request, id)).status).toBe("decided");
  const review = await (await page.request.get(`${core}/api/v1/reviews/${id}`)).json();
  expect(review.agent_note).toBe("go ahead\nafter lunch");
});

async function report(request: APIRequestContext, session: string, body: Record<string, unknown>) {
  const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, { data: body });
  expect(response.status(), await response.text()).toBe(204);
}

test("the statuses an agent reports show in its thread between the questions and reviews", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  await report(page.request, session, {
    text: "Add rate limiting",
    kind: "start",
    origin: { repo: "acme/api", ref: "main", folder: "/work/acme-api" },
  });
  // all within a second or two: the order comes from the core, not the times
  await send(page.request, asked(session, "Statuses: deploy"));
  await report(page.request, session, { text: "Running the tests" });
  await report(page.request, session, { text: "Fixing a flaky test" });
  await report(page.request, session, { kind: "done", text: "Pushed the fix" });

  await page.goto(`/#/sessions/claude-code/${session}`);
  // the task started names the session, and shows where it started
  await expect(thread(page).getByRole("heading", { level: 1 })).toHaveText("Add rate limiting");
  await expect(page.locator(".crumb-title")).toHaveText("Add rate limiting");
  await expect(thread(page).getByTestId("started")).toContainText("Started: Add rate limiting");
  const groups = thread(page).getByTestId("status-group");
  await expect(groups).toHaveCount(1);
  // two in a row: the newest, and the earlier one behind a button
  await expect(groups.last().getByTestId("status")).toHaveText([/Fixing a flaky test/]);
  await groups.last().getByRole("button", { name: "1 earlier" }).click();
  await expect(groups.last().getByTestId("status")).toHaveText([/Running the tests/, /Fixing a flaky test/]);
  await expect(thread(page).getByTestId("finished")).toContainText("Finished: Pushed the fix");
  await expect(thread(page).getByTestId("head-status")).toContainText("Finished: Pushed the fix");
  // the task in the panel, which goes to where it started
  const task = page.getByRole("region", { name: "Tasks" }).getByTestId("task");
  await expect(task).toHaveCount(1);
  await expect(task).toContainText("Add rate limiting");
  await expect(task).toHaveAccessibleName(/^Add rate limiting · Finished in /);
  await task.click();
  await expect(thread(page).getByTestId("started")).toBeInViewport();
  // the session's details, with the folder the agent works in
  const details = page.getByRole("complementary", { name: "Session details" });
  await expect(details).toContainText("acme/api");
  await expect(details).toContainText("/work/acme-api");

  // the Sessions page shows the newest status
  await page.goto("/#/sessions");
  const row = page.getByTestId("session-row").filter({ hasText: session });
  await expect(row.getByRole("link", { name: "Add rate limiting" })).toBeVisible();
  await expect(row.getByTestId("session-status")).toHaveText("Finished: Pushed the fix");
  await expect(page.getByRole("region", { name: "Sessions waiting" })).toContainText("Add rate limiting");
});

test("a status the agent reports shows in an open thread at once", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  await send(page.request, asked(session, "Live: deploy"));
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(thread(page).getByTestId("question")).toHaveCount(3);
  await expect(thread(page).getByTestId("head-status")).toHaveCount(0);
  await report(page.request, session, { text: "Waiting for the answers" });
  await expect(thread(page).getByTestId("head-status")).toContainText("Waiting for the answers");
  await expect(thread(page).getByTestId("status")).toHaveText([/Waiting for the answers/]);
});

test("a task the agent starts renames its session in the sidebar at once", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  await send(page.request, asked(session, "Rename: deploy"));
  await page.goto("/#/");
  const waiting = page.getByRole("region", { name: "Sessions waiting" }).getByTestId("waiting-session");
  await expect(waiting).toHaveText(/claude-code/);
  // after the loads the page makes as it starts
  await page.waitForTimeout(1500);
  await report(page.request, session, { text: "Ship the deploy script", kind: "start" });
  await expect(waiting).toHaveText(/Ship the deploy script/);
});
