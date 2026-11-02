// A review's rounds in a session's thread: the card stays where the first
// round was sent and names its round, and a line marks where a later round
// was sent.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core, onePayload } from "./helpers";

let n = 0;
/** A session of its own for each test. */
const fresh = () => `s-rounds-${Date.now()}-${++n}`;

const thread = (page: Page) => page.getByTestId("thread");

async function report(request: APIRequestContext, session: string, body: Record<string, unknown>) {
  const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, { data: body });
  expect(response.status(), await response.text()).toBe(204);
}

async function submit(request: APIRequestContext, session: string, title: string, revises?: string) {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "list",
      title,
      origin: { repo: "acme/api" },
      requested_by: "claude-code",
      session,
      payload: onePayload,
      revises,
    },
  });
  expect(response.status(), await response.text()).toBe(201);
  return ((await response.json()) as { id: string }).id;
}

test("a review revised in a later stage stays in its stage, with a line where the round was sent", async ({ page }) => {
  const session = fresh();
  await report(page.request, session, { text: "Write the spec", kind: "start" });
  const first = await submit(page.request, session, "Rounds: the spec");
  await report(page.request, session, { text: "Write the docs", kind: "start" });
  await submit(page.request, session, "Rounds: the docs");
  const second = await submit(page.request, session, "Rounds: the spec", first);

  await page.goto(`/#/sessions/claude-code/${session}`);
  const cards = thread(page).getByTestId("thread-review");
  // the spec's card is still the first, and shows its newest round
  await expect(cards).toHaveCount(2);
  await expect(cards.first()).toContainText("Rounds: the spec");
  await expect(cards.first()).toContainText("round 2 of 2");
  await expect(cards.first()).toHaveAttribute("href", new RegExp(`review=${second}$`));
  await expect(cards.last()).not.toContainText("round");
  // the header counts reviews, not rounds
  await expect(thread(page)).toContainText("2 reviews");

  const line = thread(page).getByTestId("revised");
  await expect(line).toHaveText(/Revised Rounds: the spec · round 2 · .* agoWaiting for you/);
  await page.locator(".thread-items").evaluate((el) => (el.scrollTop = el.scrollHeight));
  await line.click();
  await expect(cards.first()).toHaveClass(/is-flashed/);
  await expect(cards.first()).toBeInViewport();
  // and opens the newest round beside the thread
  await expect(page).toHaveURL(new RegExp(`\\?review=${second}$`));
  await page.locator("body").press("Escape");
  await expect(page.getByTestId("review-panel")).toHaveCount(0);

  // the stage that sent the spec lists it, and the later stage does not
  const stages = page.getByRole("region", { name: "Stages" }).getByTestId("stage");
  await stages.first().hover();
  await expect(page.getByRole("tooltip")).toContainText("Rounds: the spec");
  await page.mouse.move(0, 0);
  await stages.last().hover();
  await expect(page.getByRole("tooltip")).toContainText("Rounds: the docs");
  await expect(page.getByRole("tooltip")).not.toContainText("Rounds: the spec");
});

test("a round sent right after its review adds no line", async ({ page }) => {
  const session = fresh();
  const first = await submit(page.request, session, "Rounds: at once");
  await report(page.request, session, { text: "Revising" });
  await submit(page.request, session, "Rounds: at once", first);

  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(thread(page).getByTestId("thread-review")).toContainText("round 2 of 2");
  await expect(thread(page).getByTestId("status")).toHaveText([/Revising/]);
  await expect(thread(page).getByTestId("revised")).toHaveCount(0);
});
