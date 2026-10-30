import { expect, test, type APIRequestContext } from "@playwright/test";
import { core, createReview, decide } from "./helpers";

const payload = (title: string) => ({ groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, title }] }] });

/** Submits a round of a list review, revising `revises` when given. */
const submit = async (request: APIRequestContext, title: string, revises?: string) =>
  (await createReview(request, { title, revises, payload: payload(title) })).id;

const accept = (request: APIRequestContext, id: string) =>
  decide(request, id, { decisions: [{ id: 1, action: "accept" }], undecided: [] });

test("a new round of the open review shows in the switcher and says it is waiting", async ({ page }) => {
  const first = await submit(page.request, "Rounds: one");
  await accept(page.request, first);
  const second = await submit(page.request, "Rounds: two", first);
  await accept(page.request, second);

  // round 1 is open when round 3 arrives: it revises round 2, not the open one
  await page.goto(`/#/reviews/${first}`);
  const pills = page.getByRole("navigation", { name: "Rounds" }).getByRole("link");
  await expect(pills).toHaveCount(2);
  await expect(page.getByText(/Round \d+ is waiting for you/)).toHaveCount(0);

  const third = await submit(page.request, "Rounds: three", second);
  await expect(pills).toHaveCount(3);
  await expect(pills.nth(2)).toHaveClass(/is-waiting/);
  // each round in the colour of its outcome
  await expect(pills.nth(0)).toHaveClass(/\bround-decided\b/);
  await expect(pills.nth(2)).toHaveClass(/\bround-pending\b/);
  const banner = page.getByText(/Round \d+ is waiting for you/);
  await expect(banner).toContainText("Round 3 is waiting for you.");

  await banner.getByRole("link", { name: "Open it" }).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${third}$`));
  await expect(page.getByText(/Round \d+ is waiting for you/)).toHaveCount(0);
  await expect(pills.nth(2)).toHaveClass(/is-current/);

  // once opened it is no longer news: going back to round 1 says nothing
  await pills.nth(0).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${first}$`));
  await expect(pills.nth(0)).toHaveClass(/is-current/);
  await expect(page.getByText(/Round \d+ is waiting for you/)).toHaveCount(0);
  await expect(pills.nth(2)).not.toHaveClass(/is-waiting/);
});

test("⌘[ goes back, even on a round that has an earlier one", async ({ page }) => {
  // [ alone moves between rounds; with ⌘ it is Back, and only Back
  const first = await submit(page.request, "Back: one");
  await accept(page.request, first);
  const second = await submit(page.request, "Back: two", first);

  await page.goto("/#/?tab=history");
  await page.goto(`/#/reviews/${second}`);
  await expect(page.getByRole("navigation", { name: "Rounds" }).getByRole("link")).toHaveCount(2);
  await page.locator("body").click({ position: { x: 5, y: 5 } });
  const seen: string[] = [];
  page.on("framenavigated", (f) => f === page.mainFrame() && seen.push(new URL(f.url()).hash));
  await page.keyboard.press("ControlOrMeta+BracketLeft");
  // where it ends up, once both navigations had their chance: the round
  // switch and Back used to race, stepping through round 1 on the way
  await page.waitForTimeout(500);
  expect(page.url(), "⌘[ went somewhere other than back").toMatch(/#\/\?tab=history$/);
  expect(seen, "⌘[ stepped through round 1").not.toContain(`#/reviews/${first}`);
  // and Forward comes back to where Back left
  await page.keyboard.press("ControlOrMeta+BracketRight");
  await page.waitForTimeout(500);
  expect(page.url(), "⌘] did not come back").toMatch(new RegExp(`#/reviews/${second}$`));
  // [ alone still moves to the earlier round
  await page.keyboard.press("BracketLeft");
  await expect(page).toHaveURL(new RegExp(`#/reviews/${first}$`));
});

test("a view gets the whole review its round revises, with its payload and its decision", async ({ page }) => {
  const first = await submit(page.request, "Rounds: previous");
  await decide(page.request, first, { decisions: [{ id: 1, action: "reject", note: "not this one" }], undecided: [] });
  const second = await submit(page.request, "Rounds: previous", first);
  await page.goto(`/#/reviews/${second}`);
  // the list shows the previous round's verdict on the item, from its decision
  await expect(page.frameLocator("#plugin-frame").locator(".previous")).toContainText("not this one");

  // the markdown plugin compares the document with the previous round's, from its payload
  const installed = await page.request.post(`${core}/api/v1/plugins/install`, { data: { id: "forgeplane/markdown" } });
  expect(installed.status(), await installed.text()).toBe(200);
  try {
    const doc = (text: string) => ({ markdown: `# Retries\n\n${text}\n` });
    const one = await createReview(page.request, {
      title: "Rounds: a document",
      plugin: "markdown",
      payload: doc("Six attempts."),
    });
    const two = await createReview(page.request, {
      title: "Rounds: a document",
      plugin: "markdown",
      payload: doc("Eight attempts."),
      revises: one.id,
    });
    await page.goto(`/#/reviews/${two.id}`);
    const view = page.frameLocator("#plugin-frame");
    await view.locator('.modes [data-mode="changes"]').click();
    await expect(view.locator(".diff-line.del")).toContainText("Six attempts.");
  } finally {
    await page.request.delete(`${core}/api/v1/plugins/markdown`);
  }
});
