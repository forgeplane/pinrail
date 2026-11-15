// Messages the person sends the agent from its session's thread: queued
// until the agent reports a status, which hands them over, and deleted
// while they are queued; or, to an agent Pinrail runs, typed into its
// terminal, with each state shown and the buttons that go with it.

import { expect, test, type APIRequestContext, type Page } from "../fixtures";
import { core } from "../helpers";

let n = 0;
/** A session of its own for each test. */
const fresh = () => `s-message-${Date.now()}-${++n}`;

const thread = (page: Page) => page.getByTestId("thread");
const queuedList = (page: Page) => page.getByRole("region", { name: "Waiting on the agent" });

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
  await page.goto(`/#/sessions/claude-code/${session}`);
  const box = page.getByRole("textbox", { name: "Message to the agent" });
  await expect(page.getByText("The agent receives your message the next time it reports a status.")).toBeVisible();

  // a message sent by mistake, deleted while queued
  await box.fill("Never mind this one.");
  await box.press("Enter");
  const messages = thread(page).getByTestId("message");
  await expect(messages).toHaveCount(1);
  await expect(messages.first().getByTestId("message-state")).toHaveText("Queued");
  await expect(box).toHaveValue("");
  await expect(queuedList(page)).toContainText("Waiting on the agent · 1");
  await messages.first().hover();
  await messages.first().getByRole("button", { name: "Delete the message" }).click();
  await expect(messages).toHaveCount(0);
  await expect(queuedList(page)).toHaveCount(0);

  // a message the agent receives with its next status
  await box.fill("Use the v2 endpoint.\nThe v1 one is going away.");
  await page.getByTestId("send-message").click();
  await expect(messages.first()).toContainText("Use the v2 endpoint.");
  await expect(queuedList(page).getByTestId("queued-message")).toHaveCount(1);
  expect(await report(page.request, session, "Switching to v2")).toEqual([
    expect.objectContaining({ text: "Use the v2 endpoint.\nThe v1 one is going away." }),
  ]);
  await expect(messages.first().getByTestId("message-state")).toHaveText(/Delivered \d+\w ago$/);
  await expect(messages.first().getByRole("button", { name: "Delete the message" })).toHaveCount(0);
  await expect(queuedList(page)).toHaveCount(0);
  // the message sits between the status before it and the one that delivered it
  const order = await thread(page)
    .locator(".thread-items > *")
    .evaluateAll((els) =>
      els.map((el) => (el.matches('[data-testid="message"]') ? "message" : (el.textContent ?? "").trim())),
    );
  expect(order.filter((x) => x === "message" || /Reading|Switching/.test(x))).toEqual([
    expect.stringMatching(/Reading the callers/),
    "message",
    expect.stringMatching(/Switching to v2/),
  ]);
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
  await page.goto(`/#/sessions/claude-code/${session}`);
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
    await page.goto(inPanel ? `/#/sessions?open=claude-code/${session}` : `/#/sessions/claude-code/${session}`);
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
  await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: true } } });
  await page.route(new RegExp(`/api/v1/sessions/claude-code/${session}$`), async (route) => {
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
    session: { agent: "claude-code", id: session },
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
    await page.request.patch(`${core}/api/v1/settings`, { data: { labs: { runtime: false } } });
  });

  test("the box says the message goes to the agent's terminal", async ({ page }) => {
    const session = fresh();
    await report(page.request, session, "Reading the callers");
    await hosted(page, session, []);
    await page.goto(`/#/sessions/claude-code/${session}?view=summary`);
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
    await page.goto(`/#/sessions/claude-code/${session}?view=summary`);
    const states = thread(page).getByTestId("message-state");
    await expect(states).toHaveText([
      "Queued while the agent works. It is sent as your prompt when the agent finishes its turn.",
      "The agent is waiting for an answer in its terminal. Your message is sent when it is idle again.",
      "Typed into the terminal",
      "Typed into the terminal. The agent cannot confirm it.",
      /^Delivered 1m ago$/,
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
    await page.goto(`/#/sessions/claude-code/${session}?view=summary`);
    const message = thread(page).getByTestId("message");
    await expect(message.getByTestId("message-state")).toHaveText(
      "Not confirmed. The agent may not have received this message.",
    );
    await message.getByRole("button", { name: "Copy" }).click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("Use the v2 endpoint.");
    await message.getByRole("button", { name: "Retry" }).click();
    await expect.poll(() => typed.map((t) => t.id)).toEqual(["100"]);
    await expect(message.getByRole("button", { name: "Delete the message" })).toHaveCount(0);
  });
});
