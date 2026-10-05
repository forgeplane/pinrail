import { expect, test, type Page } from "@playwright/test";
import path from "node:path";
import { clearInbox, core, createReview, linkPlugin } from "./helpers";

const hello = path.resolve(__dirname, "..", "..", "plugins", "hello");

/** The bundles the review frame loads from here on, by path. */
function bundlesLoaded(page: Page) {
  const loads: string[] = [];
  page.on("framenavigated", (frame) => {
    if (frame === page.mainFrame()) return;
    const url = new URL(frame.url());
    if (url.pathname.startsWith("/bundles/")) loads.push(url.pathname);
  });
  return {
    take() {
      return loads.splice(0);
    },
  };
}

test("switching to a review of another plugin loads that plugin's view, and only that one", async ({ page }) => {
  await clearInbox(page.request);
  await linkPlugin(page.request, hello, "hello");

  const list = await createReview(page.request, {
    plugin: "list",
    title: "Switch: a list",
    payload: {
      summary: "Two proposals from the list plugin.",
      groups: [
        {
          title: "lib/acme/tickets.ex",
          items: [{ id: 1, severity: "major", title: "do_save dedups without reversing" }],
        },
      ],
    },
  });
  const question = await createReview(page.request, {
    plugin: "hello",
    title: "Switch: a question",
    payload: { message: "Push the branch to origin?" },
  });

  const loads = bundlesLoaded(page);
  await page.goto(`/#/reviews/${list.id}`);
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator("body")).toContainText("Two proposals from the list plugin.");
  loads.take();

  // The screen stays mounted across reviews. The frame for the next review
  // used to boot the plugin it had just shown, handing it a payload it could
  // not read, before the lookup for the right plugin landed.
  const rows = page.locator("[data-waiting-review]");
  for (let round = 0; round < 3; round++) {
    await rows.filter({ hasText: "Switch: a question" }).click();
    await expect(page).toHaveURL(new RegExp(`/reviews/${question.id}$`));
    await expect(frame.locator("p").first()).toHaveText("Push the branch to origin?");
    // hello is linked, and served as its folder was stored
    expect(loads.take()).toEqual([expect.stringMatching(/^\/bundles\/[0-9a-f]{64}\/view\/index\.html$/)]);

    await rows.filter({ hasText: "Switch: a list" }).click();
    await expect(page).toHaveURL(new RegExp(`/reviews/${list.id}$`));
    await expect(frame.locator("body")).toContainText("Two proposals from the list plugin.");
    // list comes with the app, a bundle named by its hash
    expect(loads.take()).toEqual([expect.stringMatching(/^\/bundles\/[0-9a-f]{64}\/view\/index\.html$/)]);
  }
});

test("a review clicked past does not come back when its fetch lands late", async ({ page }) => {
  await clearInbox(page.request);
  const first = await createReview(page.request, {
    plugin: "hello",
    title: "Quick: first",
    payload: { message: "The first question." },
  });
  const second = await createReview(page.request, {
    plugin: "list",
    title: "Quick: second",
    payload: {
      summary: "The second review, a list.",
      groups: [{ title: "a.ex", items: [{ id: 1, severity: "minor", title: "one" }] }],
    },
  });
  const third = await createReview(page.request, {
    plugin: "hello",
    title: "Quick: third",
    payload: { message: "The third question." },
  });

  await page.goto(`/#/reviews/${first.id}`);
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator("p").first()).toHaveText("The first question.");

  // the second review's answer is held back, so it lands after the third's:
  // a click past a review must not bring it back when its fetch comes in late
  let released!: () => void;
  const held = new Promise<void>((resolve) => (released = resolve));
  await page.route(`${core}/api/v1/reviews/${second.id}`, async (route) => {
    await held;
    await route.continue();
  });
  await page.evaluate((id) => (location.hash = `#/reviews/${id}`), second.id);
  await page.evaluate((id) => (location.hash = `#/reviews/${id}`), third.id);
  await expect(frame.locator("p").first()).toHaveText("The third question.");
  released();
  await page.waitForTimeout(800);
  await expect(page).toHaveURL(new RegExp(`/reviews/${third.id}$`));
  await expect(page.locator("#plugin-frame")).toHaveAttribute("title", "Quick: third");
  await expect(frame.locator("p").first()).toHaveText("The third question.");
});

test("Copy as markdown copies the review on screen, after moving from another", async ({ page }) => {
  // The screen stays mounted from one review to the next, so an action that
  // kept the id of the first would quietly copy the wrong review into a
  // merge request or a thread.
  await clearInbox(page.request);
  const payload = (summary: string) => ({
    summary,
    groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }],
  });
  const first = await createReview(page.request, {
    plugin: "list",
    title: "Copy: the first",
    payload: payload("The first review."),
  });
  const second = await createReview(page.request, {
    plugin: "list",
    title: "Copy: the second",
    payload: payload("The second review."),
  });

  await page.goto(`/#/reviews/${first.id}`);
  await expect(page.frameLocator("#plugin-frame").locator("body")).toContainText("The first review.");
  await page.locator("[data-waiting-review]").filter({ hasText: "Copy: the second" }).click();
  await expect(page.frameLocator("#plugin-frame").locator("body")).toContainText("The second review.");

  const asked = page.waitForRequest((r) => new URL(r.url()).searchParams.get("format") === "markdown");
  await page.locator("[data-copy-markdown]").click();
  expect(new URL((await asked).url()).pathname, "the markdown asked for is the review on screen").toBe(
    `/api/v1/reviews/${second.id}`,
  );
});

test("discarding a review returns to the inbox, where the others wait", async ({ page }) => {
  await clearInbox(page.request);
  const payload = (summary: string) => ({
    summary,
    groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }],
  });
  const first = await createReview(page.request, {
    plugin: "list",
    title: "Notice: the first",
    payload: payload("The first review."),
  });
  await createReview(page.request, {
    plugin: "list",
    title: "Notice: the second",
    payload: payload("The second review."),
  });

  await page.goto(`/#/reviews/${first.id}`);
  await expect(page.frameLocator("#plugin-frame").locator("body")).toContainText("The first review.");
  await page.locator("[data-discard]").click();
  await page.locator("[data-discard-confirm]").click();

  // the review has ended: back to the inbox, where the other one waits
  await expect(page).toHaveURL(/#\/$/);
  await expect(page.getByText("Discarded. The agent was told to stop.")).toBeVisible();
  const rows = page.locator("[data-review-row]");
  await expect(rows).toHaveCount(1);
  await expect(rows).toContainText("Notice: the second");
});

test("a submit the app did not ask for decides nothing", async ({ page }) => {
  // a view hands over in answer to the app's request: one that posts a
  // decision of its own accord, numbered or not, decides nothing
  await clearInbox(page.request);
  const review = await createReview(page.request, {
    plugin: "list",
    title: "Unasked: one review",
    payload: {
      summary: "Nobody asked.",
      groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }],
    },
  });
  const decisions: string[] = [];
  page.on("request", (r) => r.url().endsWith(`/reviews/${review.id}/decision`) && decisions.push(r.method()));

  await page.goto(`/#/reviews/${review.id}`);
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator("body")).toContainText("Nobody asked.");
  await frame.locator("body").evaluate(() => {
    const data = { decisions: [{ id: 1, action: "accept" }], undecided: [] };
    parent.postMessage({ pinrail: 1, type: "submit", data }, "*");
    parent.postMessage({ pinrail: 1, type: "submit", req: 1, data }, "*");
  });
  await page.waitForTimeout(500);
  expect(decisions).toEqual([]);
  const now = await (await page.request.get(`${core}/api/v1/reviews/${review.id}`)).json();
  expect(now.status).toBe("pending");
});

test("a view that answers one request twice decides the review once", async ({ page }) => {
  // the second answer must not reach the server, which would refuse it and
  // flash an error after a success
  await clearInbox(page.request);
  const review = await createReview(page.request, {
    plugin: "list",
    title: "Twice: one review",
    payload: {
      summary: "Hand over twice.",
      groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }],
    },
  });
  const decisions: string[] = [];
  page.on("request", (r) => r.url().endsWith(`/reviews/${review.id}/decision`) && decisions.push(r.method()));

  await page.goto(`/#/reviews/${review.id}`);
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator("body")).toContainText("Hand over twice.");
  // the person decides, so the view answers the request with its decision;
  // a second answer to the same request comes right after it
  await frame.locator('button[data-act="accept"][data-id="1"]').click();
  await frame.locator("body").evaluate(() => {
    const data = { decisions: [{ id: 1, action: "reject" }], undecided: [] };
    window.addEventListener("message", (event) => {
      if (event.data?.type !== "collect") return;
      const req = event.data.req;
      setTimeout(() => parent.postMessage({ pinrail: 1, type: "submit", req, data }, "*"), 0);
    });
  });
  await page.locator("[data-handover]").click();
  await expect(page.getByText("Decision recorded")).toBeVisible();
  await page.waitForTimeout(500);
  expect(decisions, "the hand-over was sent twice").toHaveLength(1);
  const decided = await (await page.request.get(`${core}/api/v1/reviews/${review.id}`)).json();
  expect(decided.decision.data.decisions).toEqual([{ id: 1, action: "accept" }]);
});

test("an error that is not JSON still says what the server answered", async ({ page }) => {
  // a proxy's HTML page, or a body cut short: the status is still news
  await clearInbox(page.request);
  const review = await createReview(page.request, {
    plugin: "list",
    title: "Not JSON: one review",
    payload: {
      summary: "Never shown.",
      groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }],
    },
  });
  await page.route(`${core}/api/v1/reviews/${review.id}`, (route) =>
    route.fulfill({ status: 502, contentType: "text/html", body: "<html><body>Bad gateway</body></html>" }),
  );
  await page.goto(`/#/reviews/${review.id}`);
  await expect(page.getByText("request failed (502)")).toBeVisible();
});

test("an unknown review is not marked viewed", async ({ page }) => {
  const posts: string[] = [];
  page.on("request", (r) => {
    if (r.method() === "POST") posts.push(new URL(r.url()).pathname);
  });
  await page.goto("/#/reviews/r_nope");
  await expect(page.getByText(/not found|no review|unknown/i).first()).toBeVisible();
  expect(posts.filter((p) => p.endsWith("/viewed"))).toEqual([]);
});
