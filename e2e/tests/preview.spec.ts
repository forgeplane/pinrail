import { expect, test } from "@playwright/test";
import { loadState } from "../helpers/state";

// A review in a browser, as an agent with a browser tool sees it: the
// plugin's view in a frame, fed the review, and a hand-over that decides it.
test("a review's preview shows its view and hands a decision over", async ({ page }) => {
  const { url } = loadState();
  const sample = await (await fetch(`${url}/api/v1/plugins/list/sample`, { method: "POST", headers: { "content-type": "application/json" }, body: "{}" })).json();

  await page.goto(`${url}/preview/reviews/${sample.id}`);
  await expect(page.locator("#title")).toHaveText(sample.title);
  const view = page.frameLocator("#view");
  await expect(view.getByText("Ecto.StaleEntryError").first()).toBeVisible();

  // the list asks again while items are undecided, through the button's label
  await page.locator("#handover").click();
  await expect(page.locator("#handover")).toHaveText("Hand over anyway");
  await page.locator("#note").fill("from the preview");
  await page.locator("#handover").click();
  await expect(page.locator("#foot")).toHaveText("Decided.");

  const decided = await (await fetch(`${url}/api/v1/reviews/${sample.id}`)).json();
  expect(decided.status).toBe("decided");
  expect(decided.agent_note).toBe("from the preview");
});

test("a preview of no review says so", async ({ page }) => {
  const { url } = loadState();
  await page.goto(`${url}/preview/reviews/r_nope`);
  await expect(page.locator("main")).toHaveText("No review r_nope.");
});
