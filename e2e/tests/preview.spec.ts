import { expect, test } from "@playwright/test";
import { loadState } from "../helpers/state";

// A review in a browser, as an agent with a browser tool sees it: the
// plugin's view in a frame, fed the review, and a hand-over that checks the
// decision and stores nothing; deciding is the app's.
test("a review's preview shows its view and checks its hand-over, deciding nothing", async ({ page }) => {
  const { url } = loadState();
  const sample = await (await fetch(`${url}/api/v1/plugins/list/sample`, { method: "POST", headers: { "content-type": "application/json" }, body: "{}" })).json();

  await page.goto(`${url}/preview/reviews/${sample.id}`);
  await expect(page.locator("#title")).toHaveText(sample.title);
  await expect(page.locator("#app")).toHaveAttribute("href", `pinrail://reviews/${sample.id}`);
  const view = page.frameLocator("#view");
  await expect(view.getByText("Ecto.StaleEntryError").first()).toBeVisible();

  // the list asks again while items are undecided, through the button's label
  await page.locator("#handover").click();
  await expect(page.locator("#handover")).toHaveText("Check: Hand over anyway");
  await page.locator("#handover").click();
  await expect(page.locator("#result p")).toHaveText(/^The decision passes\./);
  const shown = JSON.parse(await page.locator("#result pre").innerText());
  expect(shown.undecided.length).toBeGreaterThan(0);

  const after = await (await fetch(`${url}/api/v1/reviews/${sample.id}`)).json();
  expect(after.status).toBe("pending");
});

test("a preview of no review says so", async ({ page }) => {
  const { url } = loadState();
  await page.goto(`${url}/preview/reviews/r_nope`);
  await expect(page.locator("main")).toHaveText("No review r_nope.");
});
