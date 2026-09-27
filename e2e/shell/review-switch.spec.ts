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
    if (url.pathname.startsWith("/plugins/")) loads.push(url.pathname);
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

  const list = await createReview(page.request, { plugin: "list", title: "Switch: a list", payload: {
    intro: "Two proposals from the list plugin.",
    groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "major", title: "do_save dedups without reversing" }] }],
  } });
  const question = await createReview(page.request, { plugin: "hello", title: "Switch: a question", payload: { message: "Push the branch to origin?" } });

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
    expect(loads.take()).toEqual(["/plugins/hello/1/view/index.html"]);

    await rows.filter({ hasText: "Switch: a list" }).click();
    await expect(page).toHaveURL(new RegExp(`/reviews/${list.id}$`));
    await expect(frame.locator("body")).toContainText("Two proposals from the list plugin.");
    expect(loads.take()).toEqual(["/plugins/list/1/view/index.html"]);
  }
});

test("a review clicked past does not come back when its fetch lands late", async ({ page }) => {
  await clearInbox(page.request);
  const first = await createReview(page.request, { plugin: "hello", title: "Quick: first", payload: { message: "The first question." } });
  const second = await createReview(page.request, { plugin: "list", title: "Quick: second", payload: {
    intro: "The second review, a list.",
    groups: [{ title: "a.ex", items: [{ id: 1, severity: "minor", title: "one" }] }],
  } });
  const third = await createReview(page.request, { plugin: "hello", title: "Quick: third", payload: { message: "The third question." } });

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
  const payload = (intro: string) => ({ intro, groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }] });
  const first = await createReview(page.request, { plugin: "list", title: "Copy: the first", payload: payload("The first review.") });
  const second = await createReview(page.request, { plugin: "list", title: "Copy: the second", payload: payload("The second review.") });

  await page.goto(`/#/reviews/${first.id}`);
  await expect(page.frameLocator("#plugin-frame").locator("body")).toContainText("The first review.");
  await page.locator("[data-waiting-review]").filter({ hasText: "Copy: the second" }).click();
  await expect(page.frameLocator("#plugin-frame").locator("body")).toContainText("The second review.");

  const asked = page.waitForRequest((r) => new URL(r.url()).searchParams.get("format") === "markdown");
  await page.locator("[data-copy-markdown]").click();
  expect(new URL((await asked).url()).pathname, "the markdown asked for is the review on screen").toBe(`/api/v1/reviews/${second.id}`);
});

test("what the last review said stays with it: the next one opens without its notice", async ({ page }) => {
  await clearInbox(page.request);
  const payload = (intro: string) => ({ intro, groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }] });
  const first = await createReview(page.request, { plugin: "list", title: "Notice: the first", payload: payload("The first review.") });
  await createReview(page.request, { plugin: "list", title: "Notice: the second", payload: payload("The second review.") });

  await page.goto(`/#/reviews/${first.id}`);
  await expect(page.frameLocator("#plugin-frame").locator("body")).toContainText("The first review.");
  await page.locator("[data-discard]").click();
  await page.locator("[data-discard-confirm]").click();
  await expect(page.getByText("Discarded. The agent was told to stop.")).toBeVisible();

  await page.locator("[data-waiting-review]").filter({ hasText: "Notice: the second" }).click();
  await expect(page.frameLocator("#plugin-frame").locator("body")).toContainText("The second review.");
  await expect(page.getByText("Discarded. The agent was told to stop."), "the notice belongs to the review that was discarded").toHaveCount(0);
});

test("a view that posts its hand-over twice decides the review once", async ({ page }) => {
  // a click and a key press on one button, say: the second must not reach
  // the server, which would refuse it and flash an error after a success
  await clearInbox(page.request);
  const review = await createReview(page.request, { plugin: "list", title: "Twice: one review", payload: {
    intro: "Hand over twice.",
    groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }],
  } });
  const decisions: string[] = [];
  page.on("request", (r) => r.url().endsWith(`/reviews/${review.id}/decision`) && decisions.push(r.method()));

  await page.goto(`/#/reviews/${review.id}`);
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator("body")).toContainText("Hand over twice.");
  await frame.locator("body").evaluate(() => {
    const data = { decisions: [{ id: 1, action: "accept" }], undecided: [] };
    parent.postMessage({ pinrail: 1, type: "submit", data }, "*");
    parent.postMessage({ pinrail: 1, type: "submit", data }, "*");
  });
  await expect(page.getByText("Decision recorded")).toBeVisible();
  await page.waitForTimeout(500);
  expect(decisions, "the hand-over was sent twice").toHaveLength(1);
});

test("an error that is not JSON still says what the server answered", async ({ page }) => {
  // a proxy's HTML page, or a body cut short: the status is still news
  await clearInbox(page.request);
  const review = await createReview(page.request, { plugin: "list", title: "Not JSON: one review", payload: {
    intro: "Never shown.",
    groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }],
  } });
  await page.route(`${core}/api/v1/reviews/${review.id}`, (route) =>
    route.fulfill({ status: 502, contentType: "text/html", body: "<html><body>Bad gateway</body></html>" }),
  );
  await page.goto(`/#/reviews/${review.id}`);
  await expect(page.getByText("request failed (502)")).toBeVisible();
});
