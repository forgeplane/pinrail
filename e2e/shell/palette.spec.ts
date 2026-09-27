import { expect, test } from "@playwright/test";
import { clearInbox, core, createReview } from "./helpers";

test("⌘K finds every review, discarded ones included", async ({ page }) => {
  await clearInbox(page.request);
  const tag = `palette${Date.now()}`;
  const { id } = await createReview(page.request, { title: `${tag} thrown away` });
  const discarded = await page.request.post(`${core}/api/v1/reviews/${id}/discard`, { data: { reason: "not needed" } });
  expect(discarded.status(), await discarded.text()).toBe(200);

  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await palette.getByRole("textbox").fill(tag);
  await expect(palette.getByRole("option", { name: new RegExp(`${tag} thrown away`) })).toBeVisible();
});
