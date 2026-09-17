import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import path from "node:path";

const core = "http://127.0.0.1:4799";
const hello = path.resolve(__dirname, "..", "..", "plugins", "hello");

async function createReview(request: APIRequestContext, plugin: string, title: string, payload: unknown) {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: { plugin, title, origin: { repo: "acme/api", workflow: "switch" }, requested_by: "spec", payload },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { id: string };
}

async function clearInbox(request: APIRequestContext) {
  const pending = (await (await request.get(`${core}/api/v1/reviews?status=pending&limit=500`)).json()) as { id: string }[];
  for (const r of pending) await request.post(`${core}/api/v1/reviews/${r.id}/discard`, { data: { reason: "spec cleanup" } });
}

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
  const installed = await page.request.post(`${core}/api/v1/plugins/install`, { data: { source: hello, link: true } });
  expect(installed.status(), await installed.text()).toBe(202);
  await expect
    .poll(async () => ((await (await page.request.get(`${core}/api/v1/plugins`)).json()).plugins as { name: string; usable: boolean }[]).some((p) => p.name === "hello" && p.usable))
    .toBe(true);

  const list = await createReview(page.request, "list", "Switch: a list", {
    intro: "Two proposals from the list plugin.",
    allow_additions: false,
    groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "major", title: "do_save dedups without reversing" }] }],
  });
  const question = await createReview(page.request, "hello", "Switch: a question", { message: "Push the branch to origin?" });

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
    expect(loads.take()).toEqual(["/plugins/list/1/index.html"]);
  }
});

test("a review clicked past does not come back when its fetch lands late", async ({ page }) => {
  await clearInbox(page.request);
  const first = await createReview(page.request, "hello", "Quick: first", { message: "The first question." });
  const second = await createReview(page.request, "list", "Quick: second", {
    intro: "The second review, a list.",
    allow_additions: false,
    groups: [{ title: "a.ex", items: [{ id: 1, severity: "minor", title: "one" }] }],
  });
  const third = await createReview(page.request, "hello", "Quick: third", { message: "The third question." });

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
