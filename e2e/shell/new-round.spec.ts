import { expect, test, type APIRequestContext } from "@playwright/test";
import { createReview, decide } from "./helpers";

const payload = (title: string) => ({ groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, title }] }] });

/** Submits a round of a list review, revising `revises` when given. */
const submit = async (request: APIRequestContext, title: string, revises?: string) =>
  (await createReview(request, { title, revises, payload: payload(title) })).id;

const accept = (request: APIRequestContext, id: string) => decide(request, id, { decisions: [{ id: 1, action: "accept" }], undecided: [] });

test("a new round of the open review shows in the switcher and says it is waiting", async ({ page }) => {
  const first = await submit(page.request, "Rounds: one");
  await accept(page.request, first);
  const second = await submit(page.request, "Rounds: two", first);
  await accept(page.request, second);

  // round 1 is open when round 3 arrives: it revises round 2, not the open one
  await page.goto(`/#/reviews/${first}`);
  const pills = page.locator(".rounds .round-pill");
  await expect(pills).toHaveCount(2);
  await expect(page.locator("[data-new-round]")).toHaveCount(0);

  const third = await submit(page.request, "Rounds: three", second);
  await expect(pills).toHaveCount(3);
  await expect(pills.nth(2)).toHaveClass(/is-waiting/);
  // each round in the colour of its outcome
  await expect(pills.nth(0)).toHaveAttribute("data-outcome", "decided");
  await expect(pills.nth(2)).toHaveAttribute("data-outcome", "pending");
  const banner = page.locator("[data-new-round]");
  await expect(banner).toContainText("Round 3 is waiting for you.");

  await banner.getByRole("link", { name: "Open it" }).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${third}$`));
  await expect(page.locator("[data-new-round]")).toHaveCount(0);
  await expect(pills.nth(2)).toHaveClass(/is-current/);

  // once opened it is no longer news: going back to round 1 says nothing
  await pills.nth(0).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${first}$`));
  await expect(pills.nth(0)).toHaveClass(/is-current/);
  await expect(page.locator("[data-new-round]")).toHaveCount(0);
  await expect(pills.nth(2)).not.toHaveClass(/is-waiting/);
});

test("⌘[ goes back, even on a round that has an earlier one", async ({ page }) => {
  // [ alone moves between rounds; with ⌘ it is Back, and only Back
  const first = await submit(page.request, "Back: one");
  await accept(page.request, first);
  const second = await submit(page.request, "Back: two", first);

  await page.goto("/#/history");
  await page.goto(`/#/reviews/${second}`);
  await expect(page.locator(".rounds .round-pill")).toHaveCount(2);
  await page.locator("body").click({ position: { x: 5, y: 5 } });
  const seen: string[] = [];
  page.on("framenavigated", (f) => f === page.mainFrame() && seen.push(new URL(f.url()).hash));
  await page.keyboard.press("ControlOrMeta+BracketLeft");
  // where it ends up, once both navigations had their chance: the round
  // switch and Back used to race, stepping through round 1 on the way
  await page.waitForTimeout(500);
  expect(page.url(), "⌘[ went somewhere other than back").toMatch(/#\/history$/);
  expect(seen, "⌘[ stepped through round 1").not.toContain(`#/reviews/${first}`);
  // and Forward comes back to where Back left
  await page.keyboard.press("ControlOrMeta+BracketRight");
  await page.waitForTimeout(500);
  expect(page.url(), "⌘] did not come back").toMatch(new RegExp(`#/reviews/${second}$`));
  // [ alone still moves to the earlier round
  await page.keyboard.press("BracketLeft");
  await expect(page).toHaveURL(new RegExp(`#/reviews/${first}$`));
});
