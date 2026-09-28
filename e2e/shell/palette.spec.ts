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

test("a search answered late does not replace the results of the one typed after it", async ({ page }) => {
  await clearInbox(page.request);
  const tag = `late${Date.now()}`;
  for (const title of [`${tag} one`, `${tag} two`]) {
    const { id } = await createReview(page.request, { title });
    await page.request.post(`${core}/api/v1/reviews/${id}/discard`, { data: { reason: "not needed" } });
  }
  // the narrower search is held back until the broader one, typed after
  // it, has been answered
  const narrow = `${tag} one`;
  let released!: () => void;
  const held = new Promise<void>((resolve) => (released = resolve));
  await page.route(
    (url) => url.pathname === "/api/v1/reviews" && url.searchParams.get("q") === narrow,
    async (route) => {
      await held;
      await route.continue();
    },
  );
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  const box = palette.getByRole("textbox");
  await box.fill(narrow);
  await page.waitForRequest((r) => new URL(r.url()).searchParams.get("q") === narrow);
  const broad = page.waitForResponse((r) => new URL(r.url()).searchParams.get("q") === tag);
  await box.fill(tag);
  await broad;
  const two = palette.getByRole("option", { name: new RegExp(`${tag} two`) });
  await expect(two).toBeVisible();
  const late = page.waitForResponse((r) => new URL(r.url()).searchParams.get("q") === narrow);
  released();
  await late;
  // two frames: whatever the page did with the late answer is on screen
  await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
  await expect(two).toBeVisible();
  // a discarded review says how long ago it was discarded
  await expect(two).toContainText(/ago|now|\d+[smhd]/);
});
