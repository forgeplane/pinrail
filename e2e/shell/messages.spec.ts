// Messages the person sends the agent from its session's thread: queued
// until the agent reports a status, which delivers them, and deleted while
// they are queued.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core } from "./helpers";

let n = 0;
/** A session of its own for each test. */
const fresh = () => `s-message-${Date.now()}-${++n}`;

const thread = (page: Page) => page.getByTestId("thread");
const queuedList = (page: Page) => page.getByRole("region", { name: "Waiting on the agent" });

/** Reports a status as the agent, and answers the messages it delivered. */
async function report(request: APIRequestContext, session: string, text: string) {
  const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, { data: { text } });
  expect(response.status(), await response.text()).toBe(200);
  return ((await response.json()) as { messages: { text: string }[] }).messages;
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
