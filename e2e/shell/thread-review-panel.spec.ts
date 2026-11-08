// Reviews a session's thread links to, opened in a side panel beside the
// thread when reviews open in a panel, in place of the session's details.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core, onePayload, openReviewsIn } from "./helpers";

let n = 0;
/** A session of its own for each test. */
const fresh = () => `s-panel-${Date.now()}-${++n}`;

const thread = (page: Page) => page.getByTestId("thread");
const panel = (page: Page) => page.getByTestId("review-panel");
const details = (page: Page) => page.getByRole("complementary", { name: "Session details" });

async function submit(request: APIRequestContext, session: string, title: string) {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "list",
      title,
      origin: { repo: "acme/api" },
      requested_by: "claude-code",
      session,
      payload: onePayload,
    },
  });
  expect(response.status(), await response.text()).toBe(201);
  return ((await response.json()) as { id: string }).id;
}

/** A session with enough statuses that its thread scrolls, then a review. */
async function sessionWithReview(request: APIRequestContext, title: string) {
  const session = fresh();
  for (let i = 0; i < 30; i++) {
    const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
      data: { text: `Step ${i}`, kind: i % 3 ? "step" : "start" },
    });
    expect(response.status()).toBe(200);
  }
  const id = await submit(request, session, title);
  return { session, id };
}

test.afterEach(({ request }) => openReviewsIn(request, "panel"));

test("a review opens beside the thread in place of the details, and closes with Escape", async ({ page }) => {
  await openReviewsIn(page.request, "panel");
  const { session, id } = await sessionWithReview(page.request, "Panel: open and close");
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(details(page)).toBeVisible();

  const card = thread(page).getByTestId("thread-review");
  await card.click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}\\?review=${id}$`));
  await expect(panel(page)).toContainText("Panel: open and close");
  await expect(page.frameLocator("#plugin-frame").locator("body")).toContainText("One proposal.");
  await expect(details(page)).toHaveCount(0);
  await expect(card).toHaveAttribute("aria-current", "true");
  // the breadcrumb still names the session
  await expect(page.locator(".crumb-title")).toHaveText("Step 27");
  // the thread is beside it, so the review does not link to it
  await expect(panel(page).getByRole("link", { name: "Open the session's thread" })).toHaveCount(0);
  // the review takes the larger share
  const [threadBox, panelBox] = [
    await thread(page).locator(".thread-main").boundingBox(),
    await panel(page).boundingBox(),
  ];
  expect(panelBox!.width).toBeGreaterThan(threadBox!.width);

  await page.locator("body").press("Escape");
  await expect(panel(page)).toHaveCount(0);
  await expect(details(page)).toBeVisible();
  await expect(card).not.toHaveAttribute("aria-current", "true");
  await expect(page.locator(".crumb-title")).toHaveText("Step 27");

  // back opens it again, as the address holds it
  await page.goBack();
  await expect(panel(page)).toContainText("Panel: open and close");
});

test("deciding in the panel closes it and keeps the thread where it was", async ({ page }) => {
  await openReviewsIn(page.request, "panel");
  const { session } = await sessionWithReview(page.request, "Panel: decide");
  await page.goto(`/#/sessions/claude-code/${session}`);
  const items = page.locator(".thread-items");
  await thread(page).getByTestId("thread-review").click();
  await expect(panel(page)).toBeVisible();
  const scrolled = await items.evaluate((el) => el.scrollTop);
  expect(scrolled).toBeGreaterThan(0);

  const frame = page.frameLocator("#plugin-frame");
  await frame.locator('button[data-act="accept"]').first().click();
  await page.getByTestId("handover").click();
  await expect(panel(page)).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}$`));
  await expect(thread(page).getByTestId("thread-review")).toContainText(/decided/i);
  expect(Math.abs((await items.evaluate((el) => el.scrollTop)) - scrolled)).toBeLessThan(40);
});

test("Open as a page leaves the thread for the review's page", async ({ page }) => {
  await openReviewsIn(page.request, "panel");
  const { session, id } = await sessionWithReview(page.request, "Panel: expand");
  await page.goto(`/#/sessions/claude-code/${session}?review=${id}`);
  await panel(page).getByRole("button", { name: "Open as a page" }).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${id}$`));
});

test("with reviews opening on a page, a card opens the review's page", async ({ page }) => {
  await openReviewsIn(page.request, "page");
  const { session, id } = await sessionWithReview(page.request, "Panel: page");
  await page.goto(`/#/sessions/claude-code/${session}`);
  await thread(page).getByTestId("thread-review").click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${id}$`));
  await expect(page.locator(".crumb-session")).toBeVisible();
});

test("a thread in the Sessions page's panel opens a review as a page", async ({ page }) => {
  await openReviewsIn(page.request, "panel");
  const { session, id } = await sessionWithReview(page.request, "Panel: nested");
  await page.goto(`/#/sessions?q=${session}`);
  await page.getByTestId("session-row").filter({ hasText: session }).click();
  await page.getByTestId("session-panel").getByTestId("thread-review").click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${id}$`));
});

test("a card clicked in a long thread stays in place as the panel opens and closes", async ({ page, request }) => {
  await openReviewsIn(request, "panel");
  const session = fresh();
  // notes long enough to wrap again when the thread gets narrower
  const words = "The loader reads each theme file and checks it against the schema before it applies it. ";
  const note = async (i: number) => {
    const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
      data: { text: `Note ${i}. ${words.repeat(4)}`, kind: "note" },
    });
    expect(response.status()).toBe(200);
  };
  for (let i = 0; i < 8; i++) await note(i);
  await submit(request, session, "Theme loader");
  for (let i = 8; i < 16; i++) await note(i);
  await page.setViewportSize({ width: 1300, height: 800 });
  await page.goto(`/#/sessions/claude-code/${session}`);
  const card = thread(page).getByTestId("thread-review");
  await expect(card).toBeVisible();
  // bring the card to the lower part of the timeline, with notes above it
  await card.evaluate((el) => {
    const scroller = el.closest(".thread-items")!;
    scroller.scrollTop += el.getBoundingClientRect().top - scroller.getBoundingClientRect().top - 450;
  });
  const before = (await card.boundingBox())!.y;
  // WebKit scrolls by whole pixels
  const moved = async () => Math.abs((await card.boundingBox())!.y - before);

  await card.click();
  await expect(panel(page)).toBeVisible();
  await expect(details(page)).toHaveCount(0);
  await expect.poll(moved).toBeLessThanOrEqual(1);

  await page.keyboard.press("Escape");
  await expect(panel(page)).toHaveCount(0);
  await expect.poll(moved).toBeLessThanOrEqual(1);
});

test("beside an open review, the thread's lines wrap as text and never break a name or a time", async ({
  page,
  request,
}) => {
  await openReviewsIn(request, "panel");
  const session = fresh();
  const status = (text: string, kind: string) =>
    request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, { data: { text, kind } });
  await status("Attach images and files to a work item from the app (PIN-26)", "start");
  const added = await request.post(`${core}/api/v1/items`, {
    data: { repo: "acme/api", title: "Show a work item's properties as a row of pills", by: "claude-code", session },
  });
  const key = ((await added.json()) as { key: string }).key;
  await request.post(`${core}/api/v1/items/${key}/comments`, {
    data: { body: "Committed in 32fa8c4a.", by: "claude-code", session },
  });
  await status("Work item properties as a row of pills (PIN-43), committed", "done");
  await submit(request, session, "Narrow thread");
  await page.setViewportSize({ width: 1300, height: 900 });
  await page.goto(`/#/sessions/claude-code/${session}`);
  await thread(page).getByTestId("thread-review").click();
  await expect(panel(page)).toBeVisible();

  // one line box each: a name or a time never breaks inside
  const lines = (el: Element) => {
    const range = document.createRange();
    range.selectNodeContents(el);
    return new Set([...range.getClientRects()].map((r) => Math.round(r.top))).size;
  };
  const unbroken = page.locator(".msg-meta .text, .thread-age");
  await expect(unbroken.first()).toBeVisible();
  for (const el of await unbroken.all()) expect(await el.evaluate(lines), await el.textContent()).toBe(1);

  // the text of Started and Finished starts on the label's line
  for (const id of ["started", "finished"]) {
    const line = thread(page).getByTestId(id).first();
    const tops = await line.evaluate((el) => {
      const label = el.querySelector(".thread-line-label")!.getBoundingClientRect().top;
      const text = el.querySelector(".thread-line-text")!.getClientRects()[0].top;
      return Math.abs(label - text);
    });
    expect(tops, id).toBeLessThan(4);
  }
});
