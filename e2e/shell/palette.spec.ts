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

test("⌘K finds the stages agents started, and opens the thread at one", async ({ page }) => {
  await clearInbox(page.request);
  const tag = `stage${Date.now()}`;
  const session = `s-${tag}`;
  const report = async (body: Record<string, unknown>) => {
    const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
      data: { origin: { repo: "acme/api" }, ...body },
    });
    expect(response.status(), await response.text()).toBe(200);
  };
  await report({ text: `${tag} add rate limiting`, kind: "start" });
  // enough after it that the thread opened at its end would not show the start
  for (let i = 0; i < 6; i++) {
    const asked = await page.request.post(`${core}/api/v1/reviews`, {
      data: {
        kind: "question",
        title: `Set ${i}`,
        requested_by: "claude-code",
        session,
        payload: {
          questions: [
            {
              id: "q1",
              question: "Deploy to staging or production?",
              answer: "choice",
              choices: ["staging", "production"],
            },
            { id: "q2", question: "What should it be called?", answer: "text" },
          ],
        },
      },
    });
    expect(asked.status(), await asked.text()).toBe(201);
  }
  await report({ text: `${tag} document the limits`, kind: "start" });

  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await palette.getByRole("textbox").fill(`${tag} rate`);
  const found = palette.getByRole("option", { name: new RegExp(`${tag} add rate limiting`) });
  await expect(found).toBeVisible();
  await expect(palette.getByRole("option", { name: new RegExp(`${tag} document`) })).toHaveCount(0);
  await found.click();

  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}$`));
  await expect(page.getByTestId("started").filter({ hasText: `${tag} add rate limiting` })).toBeInViewport();
});
