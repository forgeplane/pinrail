// Messages the person sends the agent from its session's thread: queued at
// the top of the message box until the agent reports a status, which hands
// them over, and deleted while they are queued; or, to an agent Pinrail
// runs, typed into its terminal, with each state shown and the buttons that
// go with it. A message joins the thread where the agent received it.

import { expect, test, type APIRequestContext, type Page } from "../fixtures";
import { core, sessionId } from "../helpers";

let n = 0;
/** A session of its own for each test. */
const fresh = () => `s-message-${Date.now()}-${++n}`;

const thread = (page: Page) => page.getByTestId("thread");
const queuedList = (page: Page) => page.getByRole("region", { name: "Waiting on the agent" });
/** The messages at the top of the message box, which wait on the agent. */
const waiting = (page: Page) => page.getByTestId("waiting-message");

/** Reports a status as the agent, and answers the messages it handed
 *  over, which it acknowledges as `pinrail status` does once it printed
 *  them. */
async function report(request: APIRequestContext, session: string, text: string) {
  const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, { data: { text } });
  expect(response.status(), await response.text()).toBe(200);
  const messages = ((await response.json()) as { messages: { id: number; text: string }[] }).messages;
  if (messages.length) {
    const ack = await request.post(`${core}/api/v1/sessions/claude-code/${session}/messages/ack`, {
      data: { ids: messages.map((m) => m.id) },
    });
    expect(ack.status(), await ack.text()).toBe(204);
  }
  return messages;
}

test("a message is queued, delivered with the agent's next status, and deleted only while queued", async ({ page }) => {
  const session = fresh();
  await report(page.request, session, "Reading the callers");
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}`);
  const box = page.getByRole("textbox", { name: "Message to the agent" });
  await expect(page.getByText("The agent receives your message the next time it reports a status.")).toBeVisible();

  // a message sent by mistake waits at the top of the box, not in the
  // thread, and is deleted there
  await box.fill("Never mind this one.");
  await box.press("Enter");
  const messages = thread(page).getByTestId("message");
  await expect(waiting(page)).toHaveCount(1);
  await expect(waiting(page).getByTestId("message-state")).toHaveText(
    "Queued. The agent receives it with its next status.",
  );
  await expect(messages).toHaveCount(0);
  await expect(box).toHaveValue("");
  await expect(queuedList(page)).toContainText("Waiting on the agent · 1");
  await waiting(page).getByRole("button", { name: "Delete the message" }).click();
  await expect(waiting(page)).toHaveCount(0);
  await expect(queuedList(page)).toHaveCount(0);

  // a message the agent receives with its next status
  await box.fill("Use the v2 endpoint.\nThe v1 one is going away.");
  await page.getByTestId("send-message").click();
  await expect(waiting(page)).toContainText("Use the v2 endpoint.");
  await expect(queuedList(page).getByTestId("queued-message")).toHaveCount(1);
  expect(await report(page.request, session, "Switching to v2")).toEqual([
    expect.objectContaining({ text: "Use the v2 endpoint.\nThe v1 one is going away." }),
  ]);
  await expect(waiting(page)).toHaveCount(0);
  await expect(messages.first().getByTestId("message-state")).toHaveText(/Delivered \d+\w ago$/);
  await expect(messages.first().getByRole("button", { name: "Delete the message" })).toHaveCount(0);
  await expect(queuedList(page)).toHaveCount(0);
  // the message joins the thread where the agent received it: after the
  // status that handed it over, not where it was sent
  const order = await thread(page)
    .locator(".thread-items > *")
    .evaluateAll((els) =>
      els.map((el) => (el.matches('[data-testid="message"]') ? "message" : (el.textContent ?? "").trim())),
    );
  const switching = order.findIndex((x) => /Switching to v2/.test(x));
  expect(switching).toBeGreaterThanOrEqual(0);
  expect(order.indexOf("message")).toBeGreaterThan(switching);
});

test("a message typed but not sent stays in the box when the person leaves the session and comes back", async ({
  page,
}) => {
  const [first, other] = [fresh(), fresh()];
  await report(page.request, first, "Reading the callers");
  await report(page.request, other, "Running the tests");
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", first)}`);
  const box = page.getByRole("textbox", { name: "Message to the agent" });
  await box.fill("Also check the retry path.");

  // another page, then another session, and back each time
  const nav = page.getByRole("navigation", { name: "Main" });
  await nav.getByRole("link", { name: /Inbox/ }).click();
  await expect(page.getByRole("heading", { name: "Inbox", level: 1 })).toBeVisible();
  await page.goBack();
  await expect(box).toHaveValue("Also check the retry path.");
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", other)}`);
  await expect(box).toHaveValue("");
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", first)}`);
  await expect(box).toHaveValue("Also check the retry path.");

  // sent, it is gone from the field, and waits at the top of the box
  await box.press("Enter");
  await expect(waiting(page)).toHaveCount(1);
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", other)}`);
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", first)}`);
  await expect(waiting(page)).toHaveCount(1);
  await expect(box).toHaveValue("");
});

test("a long message waits on one line that ends in an ellipsis, inside the box", async ({ page }) => {
  const session = fresh();
  await report(page.request, session, "Reading the callers");
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}`);
  const box = page.getByRole("textbox", { name: "Message to the agent" });
  await box.fill(`Before you merge:\n\n${"Keep the old route working for the mobile app. ".repeat(12)}`);
  await box.press("Enter");
  const line = waiting(page);
  await expect(line).toHaveCount(1);
  // its Delete and when it goes stay in the box, and nothing scrolls sideways
  const [shown, frame] = [
    await line.getByRole("button", { name: "Delete the message" }).boundingBox(),
    await page.locator(".thread-composer .compose").boundingBox(),
  ];
  expect(shown!.x + shown!.width).toBeLessThanOrEqual(frame!.x + frame!.width);
  await expect(line.getByTestId("message-when")).toBeInViewport();
  expect(await page.locator(".thread-main").evaluate((el) => el.scrollWidth - el.clientWidth)).toBeLessThanOrEqual(0);
});

test("while questions wait, the box holds the comment on the answers", async ({ page }) => {
  const session = fresh();
  await report(page.request, session, "Asking");
  const asked = await page.request.post(`${core}/api/v1/reviews`, {
    data: {
      kind: "question",
      title: "Messages: ship it?",
      requested_by: "claude-code",
      session,
      payload: { questions: [{ id: "q1", question: "Ship it?", answer: "yes-no" }] },
    },
  });
  expect(asked.status()).toBe(201);
  await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}`);
  await expect(page.getByRole("textbox", { name: "Comment to the agent" })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Message to the agent" })).toHaveCount(0);
});

// The chat box floats over the bottom of a long thread; the thread's
// scrollbar, drawn inside it, stays in view down to the end of its track.
for (const inPanel of [false, true])
  test(`the chat box leaves the thread's scrollbar uncovered${inPanel ? ", in a side panel" : ""}`, async ({
    page,
    request,
  }) => {
    const session = fresh();
    for (let i = 0; i < 40; i++) await report(request, session, `Step ${i} of a long session`);
    await page.setViewportSize({ width: 1280, height: 700 });
    const id = await sessionId(page.request, "claude-code", session);
    await page.goto(inPanel ? `/#/sessions?open=claude-code/${id}` : `/#/sessions/claude-code/${id}`);
    const items = page.locator(".thread-items");
    await expect(items.getByText("Step 39 of a long session")).toBeVisible();
    const scroller = (await items.boundingBox())!;
    const footer = (await page.locator(".thread-footer").boundingBox())!;
    // the strip the scrollbar takes at the right of the scrolling list: on
    // macOS its track widens to 16px under the pointer
    expect(footer.x + footer.width).toBeLessThanOrEqual(scroller.x + scroller.width - 16);
  });

/** The session's thread with its messages replaced by `messages`, and a
 *  live run of Pinrail's in it, so that the thread shows them as an agent
 *  Pinrail runs has them. */
async function hosted(page: Page, session: string, messages: Record<string, unknown>[]) {
  // the app names the session by Pinrail's id
  const id = await sessionId(page.request, "claude-code", session);
  await page.route(new RegExp(`/api/v1/sessions/claude-code/${id}$`), async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.messages = messages.map((m, i) => ({
      id: 100 + i,
      sent_by: "pnezis",
      sent_at: new Date().toISOString(),
      delivered_at: null,
      route: "type",
      held: null,
      ...m,
    }));
    await route.fulfill({ response, json: body });
  });
  const run = {
    id: "run_1",
    host: "local",
    agent: "claude-code",
    kind: "start",
    state: "running",
    terminal: "local:t_1",
    folder: "/work",
    started_at: new Date().toISOString(),
    ended_at: null,
    end_reason: null,
    exit_code: null,
    exit_signal: null,
    session: { agent: "claude-code", id, agent_session: session },
  };
  await page.route(/\/api\/v1\/runs\?/, (route) => route.fulfill({ json: { runs: [run] } }));
  await page.route(/\/api\/v1\/terminals\/[^/]+\/tickets/, (route) => route.fulfill({ json: { ticket: "t" } }));
  const typed: { id: string; body: unknown }[] = [];
  await page.route(/\/messages\/\d+\/type$/, (route) => {
    typed.push({ id: route.request().url().split("/").at(-2)!, body: route.request().postDataJSON() });
    return route.fulfill({ status: 202, body: "" });
  });
  return typed;
}

test.describe("an agent Pinrail runs", () => {
  test.afterEach(async ({ page }) => {
    await page.unrouteAll({ behavior: "ignoreErrors" });
  });

  test("the box says the message goes to the agent's terminal", async ({ page }) => {
    const session = fresh();
    await report(page.request, session, "Reading the callers");
    await hosted(page, session, []);
    await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=summary`);
    await expect(page.getByRole("textbox", { name: "Message to the agent" })).toBeEnabled();
    await expect(page.getByText("Sends to the agent's terminal as your prompt.")).toBeVisible();
  });

  test("each message shows its state", async ({ page }) => {
    const session = fresh();
    await report(page.request, session, "Reading the callers");
    await hosted(page, session, [
      { text: "one", state: "queued", held: "working" },
      { text: "two", state: "queued", held: "needs_input" },
      { text: "three", state: "typing" },
      { text: "four", state: "typed" },
      { text: "five", state: "delivered", delivered_at: new Date(Date.now() - 60_000).toISOString() },
    ]);
    await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=summary`);
    // what the agent has not received waits at the top of the box
    await expect(waiting(page).getByTestId("message-state")).toHaveText([
      "Queued while the agent works. It is sent as your prompt when the agent finishes its turn.",
      "The agent is waiting for an answer in its terminal. Your message is sent when it is idle again.",
      "Typed into the terminal",
    ]);
    // and when each goes, in a word, beside it
    await expect(waiting(page).getByTestId("message-when")).toHaveText([
      "after this turn",
      "once it is answered",
      "being typed",
    ]);
    // what it received, where it received it, and what was typed into it
    // with no way to confirm, where it was sent, are in the thread
    await expect(thread(page).getByTestId("message").getByTestId("message-state")).toHaveText([
      /^Delivered 1m ago$/,
      "Typed into the terminal. The agent cannot confirm it.",
    ]);
    // what still waits on the agent, in a word, without the typed one that
    // cannot be confirmed
    await expect(queuedList(page).getByTestId("queued-message")).toHaveText([
      /one\s*waiting/,
      /two\s*waiting/,
      /three\s*typed/,
    ]);
  });

  test("an unconfirmed message can be retried or copied", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    const session = fresh();
    await report(page.request, session, "Reading the callers");
    const typed = await hosted(page, session, [{ text: "Use the v2 endpoint.", state: "unconfirmed" }]);
    await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=summary`);
    const message = waiting(page);
    await expect(message.getByTestId("message-state")).toHaveText(
      "Not confirmed. The agent may not have received this message.",
    );
    await message.getByRole("button", { name: "Copy" }).click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("Use the v2 endpoint.");
    await message.getByRole("button", { name: "Retry" }).click();
    await expect.poll(() => typed.map((t) => t.id)).toEqual(["100"]);
    await expect(message.getByRole("button", { name: "Delete the message" })).toHaveCount(0);
  });

  test("a message that waited a minute for the agent to start can be typed now", async ({ page }) => {
    const session = fresh();
    await report(page.request, session, "Reading the callers");
    const typed = await hosted(page, session, [
      {
        text: "Fix the login.",
        state: "queued",
        held: "starting",
        sent_at: new Date(Date.now() - 61_000).toISOString(),
      },
      { text: "Then the logout.", state: "queued", held: "starting" },
    ]);
    await page.goto(`/#/sessions/claude-code/${await sessionId(page.request, "claude-code", session)}?view=summary`);
    await expect(waiting(page).getByTestId("message-state")).toHaveText([
      "Waits for the agent to start. It is typed into the agent's terminal once the agent is idle.",
      "Waits for the agent to start. It is typed into the agent's terminal once the agent is idle.",
    ]);
    await expect(waiting(page).getByTestId("message-when")).toHaveText(["once it has started", "once it has started"]);
    // only the one that waited a minute offers it
    await expect(waiting(page).nth(1).getByRole("button", { name: "Type now" })).toHaveCount(0);
    await waiting(page).first().getByRole("button", { name: "Type now" }).click();
    await expect.poll(() => typed.map((t) => t.id)).toEqual(["100"]);
  });
});
