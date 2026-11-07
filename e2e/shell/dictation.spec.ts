import { expect, test } from "@playwright/test";
import { clearInbox, createReview, fakeRecognition, listening, openReviewsIn, speak } from "./helpers";

// Dictation, with the browser's speech recognition replaced by a fake that
// the test controls. It covers the note box's microphone and a field of a
// plugin view, for which the app runs dictation and shows it in the composer.

test("a field of the view is dictated into, and the composer says the app is listening for it", async ({ page }) => {
  await fakeRecognition(page);
  await clearInbox(page.request);
  const review = await createReview(page.request, { title: "Dictation: a note in the view" });
  await page.goto(`/#/reviews/${review.id}`);
  const frame = page.frameLocator("#plugin-frame");
  const note = frame.getByLabel("note for item 1");
  await expect(note).toBeVisible();

  await frame.getByRole("button", { name: "Dictate" }).click();
  await expect.poll(() => listening(page)).toBe(true);
  const status = page.getByRole("status").filter({ hasText: "Listening for the view" });
  await expect(status).toBeVisible();
  // the note box's own microphone stays inactive
  await expect(page.getByRole("button", { name: "Dictate the note" })).toHaveAttribute("aria-pressed", "false");

  await speak(page, "fix the typo in the moduledoc");
  await expect(note).toHaveValue("fix the typo in the moduledoc");

  await status.getByRole("button", { name: "Stop" }).click();
  await expect.poll(() => listening(page)).toBe(false);
  await expect(status).toHaveCount(0);
  await expect(page.getByText("The note travels with your decision")).toBeVisible();
});

test("the note box is dictated into from its own microphone, which stops the view's first", async ({ page }) => {
  await fakeRecognition(page);
  await clearInbox(page.request);
  const review = await createReview(page.request, { title: "Dictation: the note box" });
  await page.goto(`/#/reviews/${review.id}`);
  const frame = page.frameLocator("#plugin-frame");
  await frame.getByRole("button", { name: "Dictate" }).click();
  await expect(page.getByText("Listening for the view")).toBeVisible();

  await page.getByRole("button", { name: "Dictate the note" }).click();
  await expect(page.getByText("Listening for the view")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Stop dictating" })).toHaveAttribute("aria-pressed", "true");
  await speak(page, "looks good");
  await expect(page.getByRole("textbox", { name: "Note to the agent" })).toHaveValue("looks good");
  await page.getByRole("button", { name: "Stop dictating" }).click();
  await expect.poll(() => listening(page)).toBe(false);
});

test("Esc stops a dictation, and leaves the panel open", async ({ page }) => {
  await fakeRecognition(page);
  await clearInbox(page.request);
  const review = await createReview(page.request, { title: "Dictation: Esc" });
  // ?open= shows the review in the side panel only while reviews open there
  await openReviewsIn(page.request, "panel");
  await page.goto(`/#/?open=${review.id}`);
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.getByLabel("note for item 1")).toBeVisible();
  const panel = page.getByRole("button", { name: "Dictate the note" });

  // the note box
  await panel.click();
  await expect.poll(() => listening(page)).toBe(true);
  await page.keyboard.press("Escape");
  await expect.poll(() => listening(page)).toBe(false);
  await expect(page.getByRole("button", { name: "Dictate the note" })).toBeVisible();

  // a field of the plugin view, with Esc pressed in the app rather than in the frame
  await frame.getByRole("button", { name: "Dictate" }).click();
  await expect(page.getByText("Listening for the view")).toBeVisible();
  await page.getByRole("textbox", { name: "Note to the agent" }).focus();
  await page.keyboard.press("Escape");
  await expect(page.getByText("Listening for the view")).toHaveCount(0);
  await expect(frame.getByLabel("note for item 1")).toBeVisible();
});
