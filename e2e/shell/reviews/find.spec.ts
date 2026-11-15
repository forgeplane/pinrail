import { expect, test } from "../fixtures";
import { clearInbox, createReview } from "../helpers";

test("⌘F finds text in the view, from the page or the view, and Escape clears it", async ({ page }) => {
  await clearInbox(page.request);
  const review = await createReview(page.request, {
    plugin: "list",
    title: "Find: in the view",
    payload: {
      summary: "Two proposals about the ledger.",
      groups: [
        {
          title: "lib/acme/tickets.ex",
          items: [
            { id: 1, severity: "major", title: "Ledger totals drift on refunds" },
            { id: 2, severity: "minor", title: "moduledoc typo in the ledger" },
          ],
        },
      ],
    },
  });
  await page.goto(`/#/reviews/${review.id}`);
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator("body")).toContainText("moduledoc typo");
  const bar = page.getByRole("search", { name: "Find in the view" });
  const field = bar.getByRole("textbox", { name: "Find in the view" });
  const count = bar.getByRole("status");
  const highlighted = () =>
    frame.locator("body").evaluate(() => (CSS as any).highlights.get("pinrail-find")?.size ?? 0);

  await page.getByTestId("top-bar").click();
  await page.keyboard.press("ControlOrMeta+f");
  await expect(field).toBeFocused();
  // every time the word shows: the summary, and each title in the list and
  // in its rail
  const n = await frame.locator("body").evaluate((body) => body.innerText.match(/ledger/gi)!.length);
  expect(n).toBeGreaterThan(2);
  await field.fill("ledger");
  await expect(count).toHaveText(`1 of ${n}`);
  expect(await highlighted()).toBe(n);
  await field.press("Enter");
  await expect(count).toHaveText(`2 of ${n}`);
  await field.press("Shift+Enter");
  await field.press("Shift+Enter");
  await expect(count).toHaveText(`${n} of ${n}`);
  await bar.getByRole("button", { name: "Next match" }).click();
  await expect(count).toHaveText(`1 of ${n}`);

  await field.fill("nowhere in it");
  await expect(count).toHaveText("No matches");
  await expect(bar.getByRole("button", { name: "Next match" })).toBeDisabled();

  await field.press("Escape");
  await expect(bar).toHaveCount(0);
  expect(await highlighted()).toBe(0);

  // with the view holding the keyboard, the key goes up to the app, and the
  // bar comes back with the last search selected
  await frame.locator("body").click();
  await page.keyboard.press("ControlOrMeta+f");
  await expect(field).toBeFocused();
  await expect(field).toHaveValue("nowhere in it");
  await field.fill("typo");
  await expect(count).toHaveText(/^1 of \d+$/);
});
