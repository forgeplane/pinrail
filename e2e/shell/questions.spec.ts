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
/** A session's row in the sidebar. */
const inSidebar = (page: Page, session: string) =>
  page.getByTestId("sidebar-session").and(page.locator(`[href="#/sessions/claude-code/${session}"]`));

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
  await expect(rows.first()).toContainText("1 question waiting");
  await expect(rows.filter({ hasText: answered })).toContainText("1 question");

  await page.getByRole("searchbox", { name: "Search sessions" }).fill(answered);
  await expect(rows).toHaveCount(1);
  await rows.first().click();
  await expect(page).toHaveURL(new RegExp(`/sessions\\?.*open=claude-code%2F${answered}`));
  await expect(page.getByTestId("session-panel").getByTestId("answer")).toHaveCount(3);
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

  const waiting = inSidebar(page, session);
  await expect(waiting).toContainText("claude-code");
  await expect(waiting.getByTestId("sidebar-session-waiting")).toHaveText("1");
  await waiting.click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}$`));

  // once nothing waits in it, the session stays without its count
  await clearInbox(page.request);
  await expect(waiting.getByTestId("sidebar-session-waiting")).toHaveCount(0);
  await expect(waiting).toBeVisible();
});

test("with nothing waiting, the chat box takes a message to the agent of the session", async ({ page }) => {
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
  await expect(page.getByRole("textbox", { name: "Message to the agent" })).toBeEditable();
  await expect(page.getByTestId("send-answers")).toHaveCount(0);
});

test("with nothing waiting and no session, the chat box cannot be written in", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  await clearInbox(page.request);
  const { id } = await send(page.request, asked(null, "Idle: deploy"));
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
  expect(response.status(), await response.text()).toBe(200);
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
  // the stage started names the session, and shows where it started
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
  // with its questions waiting, the header says so rather than Finished
  await expect(thread(page).getByTestId("head-finished")).toHaveCount(0);
  // the stage in the panel, which goes to where it started
  const stage = page.getByRole("region", { name: "Stages" }).getByTestId("stage");
  await expect(stage).toHaveCount(1);
  await expect(stage).toContainText("Add rate limiting");
  await expect(stage).toHaveAccessibleName(/^Add rate limiting · Finished in /);
  // the time beside it is how long it took
  await expect(stage.locator(".thread-rail-when")).toHaveText(/^\d+s$/);
  await stage.click();
  await expect(thread(page).getByTestId("started")).toBeInViewport();
  // the session's details, with the folder the agent works in
  const details = page.getByRole("complementary", { name: "Session details" });
  await expect(details).toContainText("acme/api");
  await expect(details).toContainText("/work/acme-api");

  // the Sessions page shows the newest status
  await page.goto("/#/sessions");
  const row = page.getByTestId("session-row").filter({ hasText: session });
  await expect(row.locator(".review-row-title")).toContainText("Add rate limiting");
  await expect(row.getByTestId("session-status")).toHaveText("Finished: Pushed the fix");
  await expect(inSidebar(page, session)).toContainText("Add rate limiting");
});

test("a status the agent reports shows in an open thread at once", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  await send(page.request, asked(session, "Live: deploy"));
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(thread(page).getByTestId("question")).toHaveCount(3);
  await expect(thread(page).getByTestId("status")).toHaveCount(0);
  await report(page.request, session, { text: "Waiting for the answers" });
  await expect(thread(page).getByTestId("status")).toHaveText([/Waiting for the answers/]);
  // once nothing waits, a session the agent finished says so beside its title
  await report(page.request, session, { kind: "done" });
  await clearInbox(page.request);
  await expect(thread(page).getByTestId("head-finished")).toHaveText("Finished");
  await expect(thread(page).getByTestId("status")).toHaveText([/Waiting for the answers/]);
});

test("a stage the agent starts renames its session in the sidebar at once", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  await send(page.request, asked(session, "Rename: deploy"));
  await page.goto("/#/");
  const waiting = inSidebar(page, session);
  await expect(waiting).toHaveText(/claude-code/);
  // after the loads the page makes as it starts
  await page.waitForTimeout(1500);
  await report(page.request, session, { text: "Ship the deploy script", kind: "start" });
  await expect(waiting).toHaveText(/Ship the deploy script/);
});

test("a session's details hide, show and resize, and stay so", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  await send(page.request, asked(session, "Details: deploy"));
  await page.goto(`/#/sessions/claude-code/${session}`);
  const details = page.getByRole("complementary", { name: "Session details" });
  await expect(details).toBeVisible();

  // wider with the edge's arrow keys
  const before = (await details.boundingBox())!.width;
  await page.getByRole("separator", { name: "Resize the session's details" }).focus();
  await page.keyboard.press("Shift+ArrowLeft");
  await expect.poll(async () => (await details.boundingBox())!.width).toBeGreaterThan(before + 60);

  // hidden with the button, and still hidden after a reload
  await page.getByRole("button", { name: "Hide the session's details" }).click();
  await expect(details).toHaveCount(0);
  await page.reload();
  await expect(thread(page).getByTestId("question")).toHaveCount(3);
  await expect(details).toHaveCount(0);
  // shown again with the keys, as wide as it was
  await page.keyboard.press(process.platform === "darwin" ? "Meta+Shift+B" : "Control+Shift+B");
  await expect(details).toBeVisible();
  expect((await details.boundingBox())!.width).toBeGreaterThan(before + 60);
});

test("a session's details list its ended reviews, folded until opened", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  const review = (title: string) => ({
    plugin: "list",
    title,
    origin: { repo: "acme/api" },
    requested_by: "claude-code",
    session,
    payload: onePayload,
  });
  const { id } = await send(page.request, review("Handled: the schema"));
  await send(page.request, review("Handled: still waiting"));
  await send(page.request, asked(session, "Handled: deploy"));
  const discarded = await page.request.post(`${core}/api/v1/reviews/${id}/discard`, { data: { reason: "not now" } });
  expect(discarded.status()).toBe(200);

  await page.goto(`/#/sessions/claude-code/${session}`);
  const handled = page.getByRole("region", { name: "Reviews", exact: true });
  const fold = handled.getByRole("button", { name: "Reviews · 1" });
  await expect(fold).toHaveAttribute("aria-expanded", "false");
  await expect(handled.getByTestId("handled-review")).toHaveCount(0);
  await fold.click();
  // the review that ended, not the one waiting nor the questions
  await expect(handled.getByTestId("handled-review")).toHaveText([/Handled: the schema/]);
  await page.reload();
  await expect(handled.getByTestId("handled-review")).toHaveCount(1);
  await handled.getByTestId("handled-review").click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}\\?review=${id}$`));
});

test("a session opens in the side panel beside the Sessions page", async ({ page }) => {
  await clearInbox(page.request);
  const first = fresh();
  const second = fresh();
  await send(page.request, asked(first, "Panel: first"));
  await send(page.request, asked(second, "Panel: second"));
  await page.goto("/#/sessions");
  const sessionRow = (id: string) => page.getByTestId("session-row").filter({ hasText: id });
  const panel = page.getByTestId("session-panel");

  // a click opens the session's thread beside the list, which stays
  await sessionRow(second).click();
  await expect(panel.getByTestId("thread")).toBeVisible();
  await expect(panel.getByTestId("question")).toHaveCount(3);
  await expect(panel.getByRole("link", { name: `session ${second}` })).toBeVisible();
  await expect(sessionRow(second)).toHaveClass(/is-open/);
  await expect(page).toHaveURL(/\/sessions\?open=claude-code%2F/);
  // beside the panel, the list hides what the session holds, and keeps its id on one line
  await expect(sessionRow(second).locator(".session-row-total")).toBeHidden();
  const id = sessionRow(second).getByTitle(second);
  const line = await id.evaluate((el) => parseFloat(getComputedStyle(el).lineHeight));
  expect((await id.boundingBox())!.height).toBeLessThan(line * 1.5);

  // the details are hidden at first, and shown with the button
  const details = panel.getByRole("complementary", { name: "Session details" });
  await expect(details).toHaveCount(0);
  await panel.getByRole("button", { name: "Show the session's details" }).click();
  await expect(details).toContainText("acme/api");

  // J opens the next session, with the details as they were, and Esc closes the panel
  await page.keyboard.press("j");
  await expect(sessionRow(first)).toHaveClass(/is-open/);
  await expect(panel.getByRole("link", { name: `session ${first}` })).toBeVisible();
  await expect(details).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(panel).toHaveCount(0);

  // the panel's session opens as a page
  await sessionRow(second).click();
  await panel.getByRole("button", { name: "Open as a page" }).click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${second}$`));
  await page.goBack();

  // with reviews opening on their own page, so do sessions
  await openReviewsIn(page.request, "page");
  await page.reload();
  await sessionRow(first).click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${first}$`));
});

test("the Sessions page filters the sessions by project", async ({ page }) => {
  await clearInbox(page.request);
  const inApi = fresh();
  const inWeb = fresh();
  const nowhere = fresh();
  await send(page.request, asked(inApi, "Project: api"));
  await send(page.request, { ...asked(inWeb, "Project: web"), origin: { repo: "acme/web" } });
  await send(page.request, { ...asked(nowhere, "Project: none"), origin: {} });
  await page.goto("/#/sessions");
  const rows = page.getByTestId("session-row");
  const choose = async (name: string) => {
    await page.getByRole("main").getByRole("button", { name: "Project" }).click();
    await page.getByRole("option", { name, exact: true }).click();
  };

  await choose("acme/web");
  await expect(page).toHaveURL(/repo=acme%2Fweb/);
  await expect(rows.filter({ hasText: inWeb })).toHaveCount(1);
  await expect(rows.filter({ hasText: inApi })).toHaveCount(0);
  await expect(rows.filter({ hasText: nowhere })).toHaveCount(0);

  await choose("No project");
  await expect(rows.filter({ hasText: nowhere })).toHaveCount(1);
  await expect(rows.filter({ hasText: inWeb })).toHaveCount(0);

  await choose("All projects");
  await expect(rows.filter({ hasText: inApi })).toHaveCount(1);
  await expect(rows.filter({ hasText: inWeb })).toHaveCount(1);
});

test("a review's header links to the thread of its session", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  const sent = (title: string, from: string | null) =>
    send(page.request, {
      plugin: "list",
      title,
      origin: { repo: "acme/api" },
      requested_by: "claude-code",
      session: from,
      payload: onePayload,
    });
  const { id } = await sent("Header: in a session", session);
  const { id: alone } = await sent("Header: on its own", null);
  const link = page.getByRole("link", { name: "Open the session's thread" });

  // on its page, the link in the bar opens the session's thread
  await page.goto(`/#/reviews/${id}`);
  await link.click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}$`));
  await expect(thread(page).getByTestId("thread-review")).toContainText("Header: in a session");

  // in the inbox's side panel, the panel's header has it too
  await page.goto(`/#/?open=${id}`);
  await expect(page.getByTestId("panel-head").getByRole("link", { name: "Open the session's thread" })).toBeVisible();

  // a review sent from no session has none
  await page.goto(`/#/reviews/${alone}`);
  await expect(page.getByTestId("review-title")).toHaveText("Header: on its own");
  await expect(link).toHaveCount(0);
});

test("J on the Sessions page scrolls the focused session into view", async ({ page }) => {
  await clearInbox(page.request);
  await page.setViewportSize({ width: 1100, height: 500 });
  for (let i = 0; i < 14; i++) await send(page.request, asked(fresh(), `Scroll: ${i}`));
  await page.goto("/#/sessions");
  const rows = page.getByTestId("session-row");
  await expect(rows.nth(13)).toBeAttached();
  for (let i = 0; i < 13; i++) await page.keyboard.press("j");
  await expect(rows.nth(13)).toHaveClass(/is-focused/);
  await expect(rows.nth(13)).toBeInViewport();
});

test("a question's text shows inline Markdown, and its labels read it as text", async ({ page }) => {
  await clearInbox(page.request);
  const session = fresh();
  await send(page.request, {
    kind: "question",
    title: "Markdown: endpoint",
    requested_by: "claude-code",
    session,
    payload: {
      questions: [{ id: "q1", question: "Keep `GET /api/v1/reviews` for **one** release?", answer: "yes-no" }],
    },
  });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const line = question(page, "release").locator(".msg-question");
  await expect(line.locator("code")).toHaveText("GET /api/v1/reviews");
  await expect(line.locator("strong")).toHaveText("one");
  await expect(line).not.toContainText("`");
  // the choices are named by the question as it reads, without the marks
  await expect(page.getByRole("radiogroup", { name: "Keep GET /api/v1/reviews for one release?" })).toBeVisible();
});

test("a review card in the thread keeps its title in view beside many counts", async ({ page }) => {
  const session = fresh();
  const items = ["blocker", "major", "minor", "nit"].flatMap((severity, g) =>
    [1, 2].map((k) => ({ id: g * 2 + k, severity, title: `${severity} ${k}` })),
  );
  await send(page.request, {
    plugin: "list",
    title: "Card: a review with many counts",
    origin: { repo: "acme/api" },
    requested_by: "claude-code",
    session,
    payload: { summary: "Eight proposals.", groups: [{ title: "lib/acme.ex", items }] },
  });
  await page.setViewportSize({ width: 1000, height: 700 });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const card = thread(page).getByTestId("thread-review");
  await expect(card.getByTestId("summary-count")).toHaveCount(4);
  // the title has most of the card, and the line under it stays one line
  const box = await card.boundingBox();
  const title = await card.locator(".thread-card-title").boundingBox();
  expect((title?.width ?? 0) / (box?.width ?? 1)).toBeGreaterThan(0.5);
  const meta = await card.locator(".thread-card-meta").boundingBox();
  expect(meta?.height ?? 99).toBeLessThan(24);
});

test("every stage shows how long it took and, in its tooltip, when it started and what it sent", async ({ page }) => {
  const session = fresh();
  await report(page.request, session, { text: "Plan the work", kind: "start", origin: { repo: "acme/api" } });
  await send(page.request, {
    plugin: "list",
    title: "Stage: the plan",
    origin: { repo: "acme/api" },
    requested_by: "claude-code",
    session,
    payload: onePayload,
  });
  await send(page.request, asked(session, "Stage: a question"));
  await report(page.request, session, { text: "Do the work", kind: "start" });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const stages = page.getByRole("region", { name: "Stages" }).getByTestId("stage");
  await expect(stages).toHaveCount(2);
  // a stage the agent never finished took until the next one started, and the current one runs on
  await expect(stages.nth(0).locator(".thread-rail-when")).toHaveText(/^\d+s$/);
  await expect(stages.nth(1).locator(".thread-rail-when")).toHaveText(/^\d+s$/);
  // when each started is in its name
  await expect(stages.nth(0)).toHaveAccessibleName(/Started /);
  // and in the tooltip, under how long it took
  await stages.nth(0).hover();
  await expect(page.getByRole("tooltip")).toContainText("Plan the work");
  await expect(page.getByRole("tooltip")).toContainText(/Took \d+s/);
  await expect(page.getByRole("tooltip")).toContainText("Started ");
  // with the reviews the stage sent, and not its questions
  await expect(page.getByRole("tooltip")).toContainText("Stage: the plan");
  await expect(page.getByRole("tooltip")).toContainText("waiting");
  await expect(page.getByRole("tooltip")).not.toContainText("Stage: a question");
});
