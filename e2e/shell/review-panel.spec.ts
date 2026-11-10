import { expect, test, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { clearInbox, createReview, openReviewsIn, settingsDialog } from "./helpers";

const panel = (page: Page) => page.getByTestId("review-panel");
const row = (page: Page, title: string) => page.getByTestId("review-row").filter({ hasText: title });

test.afterEach(({ request }) => openReviewsIn(request, "panel"));

test("Escape closes the panel from inside the view too, once the view leaves the key alone", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Panel: Escape in the view" });
  await page.goto("/#/");
  await row(page, "Panel: Escape in the view").click();
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator("body")).toContainText("One proposal.");
  await frame.locator("body").click();
  await page.keyboard.press("Escape");
  await expect(panel(page)).toHaveCount(0);
});

test("a code review closes with Escape when it has nothing of its own to cancel", async ({ page }) => {
  await clearInbox(page.request);
  const fixture = JSON.parse(
    fs.readFileSync(path.join(__dirname, "..", "..", "plugins", "code-review", "fixtures", "webhooks.json"), "utf8"),
  );
  await createReview(page.request, {
    title: "Panel: Escape in a code review",
    plugin: "code-review",
    payload: fixture.payload,
  });
  await page.goto("/#/");
  const frame = page.frameLocator("#plugin-frame");

  // with the app in focus
  await row(page, "Panel: Escape in a code review").click();
  await expect(frame.locator(".card").first()).toBeVisible();
  await page.locator("body").press("Escape");
  await expect(panel(page)).toHaveCount(0);

  // in the frame, the first Escape closes the open note and the second the panel
  await row(page, "Panel: Escape in a code review").click();
  await frame.locator(".card").first().getByRole("button", { name: "Add instruction" }).click();
  await expect(frame.locator("textarea[data-note-ta]")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(frame.locator("textarea[data-note-ta]")).toHaveCount(0);
  await expect(panel(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(panel(page)).toHaveCount(0);
});

test("a view's key pressed in the app gives the view the keyboard, so c in a code review takes typing at once", async ({
  page,
}) => {
  await clearInbox(page.request);
  const fixture = JSON.parse(
    fs.readFileSync(path.join(__dirname, "..", "..", "plugins", "code-review", "fixtures", "webhooks.json"), "utf8"),
  );
  await createReview(page.request, { title: "Panel: c to comment", plugin: "code-review", payload: fixture.payload });
  await page.goto("/#/");
  await row(page, "Panel: c to comment").click();
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator(".card").first()).toBeVisible();
  // the app has the focus, as after a click in the list
  await page.locator("body").press("c");
  await expect(frame.locator("textarea[data-note-ta]")).toBeVisible();
  await page.keyboard.type("shorter, please");
  await expect(frame.locator("textarea[data-note-ta]")).toHaveValue("shorter, please");
});

test("a review opens beside the inbox, and closes with its button or Escape", async ({ page }) => {
  await clearInbox(page.request);
  const review = await createReview(page.request, { title: "Panel: open and close" });

  await page.goto("/#/");
  await row(page, "Panel: open and close").click();
  await expect(page).toHaveURL(new RegExp(`#/\\?open=${review.id}$`));
  await expect(panel(page).getByTestId("review-title")).toHaveText("Panel: open and close");
  await expect(page.frameLocator("#plugin-frame").locator("body")).toContainText("One proposal.");
  // the list stays visible, with the open review's row marked
  await expect(row(page, "Panel: open and close")).toHaveAttribute("aria-current", "true");

  await panel(page).getByRole("button", { name: "Close the panel" }).click();
  await expect(panel(page)).toHaveCount(0);
  await expect(page).toHaveURL(/#\/$/);

  await row(page, "Panel: open and close").click();
  // the view has loaded, as when a person reads the review before closing it
  await expect(page.frameLocator("#plugin-frame").locator("body")).toContainText("One proposal.");
  await page.locator("body").press("Escape");
  await expect(panel(page)).toHaveCount(0);
});

test("in a narrow window, the panel covers the whole list", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Panel: narrow window" });
  await page.setViewportSize({ width: 860, height: 700 });
  await page.goto("/#/");
  await row(page, "Panel: narrow window").click();
  await expect(panel(page).getByTestId("review-title")).toHaveText("Panel: narrow window");
  const split = await panel(page).locator("xpath=..").boundingBox();
  const box = await panel(page).boundingBox();
  expect(box!.x).toBeCloseTo(split!.x, 0);
  expect(box!.width).toBeCloseTo(split!.width, 0);
});

test("a decision in the panel moves on to the next review, and the last one closes it", async ({ page }) => {
  await clearInbox(page.request);
  // newest first: Second, then First
  await createReview(page.request, { title: "Panel: first" });
  await createReview(page.request, { title: "Panel: second" });
  const frame = page.frameLocator("#plugin-frame");

  await page.goto("/#/");
  await row(page, "Panel: second").click();
  await expect(frame.locator("body")).toContainText("One proposal.");
  await frame.locator('button[data-act="accept"]').first().click();
  await page.getByTestId("handover").click();

  await expect(page.getByText("Decision recorded")).toBeVisible();
  await expect(panel(page).getByTestId("review-title")).toHaveText("Panel: first");
  await expect(page.getByTestId("review-row")).toHaveCount(1);

  await expect(frame.locator("body")).toContainText("One proposal.");
  await frame.locator('button[data-act="accept"]').first().click();
  await page.getByTestId("handover").click();
  await expect(panel(page)).toHaveCount(0);
  await expect(page).toHaveURL(/#\/$/);
});

test("⌥↓ and ⌥↑ walk the list from the panel, in the order it shows", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Panel walk: oldest" });
  await createReview(page.request, { title: "Panel walk: newest" });

  await page.goto("/#/");
  await row(page, "Panel walk: newest").click();
  await expect(panel(page).getByTestId("review-title")).toHaveText("Panel walk: newest");
  await page.locator("body").press("Alt+ArrowDown");
  await expect(panel(page).getByTestId("review-title")).toHaveText("Panel walk: oldest");
  await page.locator("body").press("Alt+ArrowUp");
  await expect(panel(page).getByTestId("review-title")).toHaveText("Panel walk: newest");
});

test("⌥↓ and ⌥↑ leave a text field and an open dialog alone", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Typing walk: oldest" });
  await createReview(page.request, { title: "Typing walk: newest" });
  await page.goto("/#/");
  const search = page.getByRole("searchbox", { name: "Search inbox" });

  // in a field, with no review open: the keys move the caret
  await search.focus();
  await page.keyboard.press("Alt+ArrowDown");
  await page.keyboard.press("Alt+ArrowUp");
  await expect(page).toHaveURL(/#\/$/);
  await expect(search).toBeFocused();

  // in a field, with a review open in the panel: the same
  await row(page, "Typing walk: newest").click();
  await expect(panel(page).getByTestId("review-title")).toHaveText("Typing walk: newest");
  await search.focus();
  await page.keyboard.press("Alt+ArrowDown");
  await expect(panel(page).getByTestId("review-title")).toHaveText("Typing walk: newest");

  // in a dialog: the screen behind it stays
  await page.locator("body").click();
  await page.keyboard.press("ControlOrMeta+k");
  await expect(page.getByRole("dialog", { name: "Search" })).toBeVisible();
  await page.keyboard.press("Alt+ArrowDown");
  await expect(panel(page).getByTestId("review-title")).toHaveText("Typing walk: newest");
});

test("J and K move to the next and previous review, from the list and from the view", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Panel keys: oldest" });
  await createReview(page.request, { title: "Panel keys: newest" });
  const title = panel(page).getByTestId("review-title");
  const frame = page.frameLocator("#plugin-frame");

  await page.goto("/#/");
  await row(page, "Panel keys: newest").click();
  await expect(title).toHaveText("Panel keys: newest");
  await page.locator("body").press("j");
  await expect(title).toHaveText("Panel keys: oldest");
  // at the end of the list, j wraps around to the first review, and k back
  await page.locator("body").press("j");
  await expect(title).toHaveText("Panel keys: newest");
  await page.locator("body").press("k");
  await expect(title).toHaveText("Panel keys: oldest");

  // with the frame in focus, the view forwards the keys to the app
  await expect(frame.locator("body")).toContainText("One proposal.");
  await frame.locator("body").click();
  await page.keyboard.press("k");
  await expect(title).toHaveText("Panel keys: newest");
});

test("the panel is one step back however many reviews it went through", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Panel back: oldest" });
  await createReview(page.request, { title: "Panel back: middle" });
  await createReview(page.request, { title: "Panel back: newest" });

  await page.goto("/#/");
  await row(page, "Panel back: newest").click();
  await row(page, "Panel back: middle").click();
  await expect(panel(page).getByTestId("review-title")).toHaveText("Panel back: middle");
  await page.locator("body").press("Alt+ArrowDown");
  await expect(panel(page).getByTestId("review-title")).toHaveText("Panel back: oldest");
  // go back with ⌘[, which does the same as the web view's ⌘←
  await page.locator("body").press("ControlOrMeta+BracketLeft");
  await expect(page).toHaveURL(/#\/$/);
  await expect(panel(page)).toHaveCount(0);
});

test("a second row clicked before the panel shows replaces the first in the history", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Panel quick: oldest" });
  await createReview(page.request, { title: "Panel quick: newest" });

  await page.goto("/#/");
  await expect(row(page, "Panel quick: oldest")).toBeVisible();
  // both clicks in one task, so the inbox cannot render between them
  await page.evaluate(() => {
    const rows = [...document.querySelectorAll<HTMLElement>('[data-testid="review-row"]')];
    const find = (title: string) => rows.find((r) => r.textContent?.includes(title))!;
    find("Panel quick: newest").click();
    find("Panel quick: oldest").click();
  });
  await expect(panel(page).getByTestId("review-title")).toHaveText("Panel quick: oldest");
  await page.locator("body").press("ControlOrMeta+BracketLeft");
  await expect(page).toHaveURL(/#\/$/);
  await expect(panel(page)).toHaveCount(0);
});

test("Enter on the row lit opens the panel one step from the list", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Panel enter: one" });
  await createReview(page.request, { title: "Panel enter: two" });

  await page.goto("/#/");
  await expect(page.getByTestId("review-row")).toHaveCount(2);
  await page.keyboard.press("j");
  await expect(page.locator(".review-row.is-focused")).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(panel(page).getByTestId("review-title")).toHaveText(/Panel enter/);
  // one step back closes it: Enter added one entry to the history
  await page.locator("body").press("ControlOrMeta+BracketLeft");
  await expect(page).toHaveURL(/#\/$/);
  await expect(panel(page)).toHaveCount(0);
});

test("walking from the panel moves the highlight off the row under the pointer", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Panel highlight: oldest" });
  await createReview(page.request, { title: "Panel highlight: newest" });

  await page.goto("/#/");
  // the clicked row is highlighted, and the pointer stays over it
  await row(page, "Panel highlight: newest").click();
  await expect(row(page, "Panel highlight: newest")).toHaveClass(/is-focused/);
  await page.locator("body").press("Alt+ArrowDown");
  await expect(panel(page).getByTestId("review-title")).toHaveText("Panel highlight: oldest");
  await expect(row(page, "Panel highlight: oldest")).toHaveClass(/is-focused/);
  await expect(row(page, "Panel highlight: newest")).not.toHaveClass(/is-focused/);
});

test("the panel opens a review as a page, and the page goes back to the panel", async ({ page }) => {
  await clearInbox(page.request);
  const review = await createReview(page.request, { title: "Panel: as a page" });

  await page.goto("/#/");
  await row(page, "Panel: as a page").click();
  await panel(page).getByRole("button", { name: "Open as a page" }).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${review.id}$`));
  await expect(panel(page)).toHaveCount(0);

  await page.getByRole("button", { name: "Show in the inbox's panel" }).click();
  await expect(page).toHaveURL(new RegExp(`#/\\?open=${review.id}$`));
  await expect(panel(page)).toBeVisible();
});

test("with reviews opening as pages, set in Settings, a row opens the page", async ({ page }) => {
  await clearInbox(page.request);
  const review = await createReview(page.request, { title: "Panel: turned off" });

  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await settingsDialog(page).getByRole("radio", { name: "As a page" }).click();
  await page.keyboard.press("Escape");
  await expect(settingsDialog(page)).toHaveCount(0);

  await row(page, "Panel: turned off").click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${review.id}$`));
  await expect(page.getByRole("button", { name: "Show in the inbox's panel" })).toHaveCount(0);
});

test("a panel closed while its edge is dragged leaves the page as it was", async ({ page, request }) => {
  await openReviewsIn(request, "panel");
  await clearInbox(request);
  await createReview(request, { title: "Drag me" });
  await page.goto("/#/");
  await row(page, "Drag me").click();
  await expect(panel(page)).toBeVisible();
  // the drag starts on the panel's edge, and the panel closes before it ends
  const edge = page.getByRole("separator", { name: "Resize the review panel" });
  const box = (await edge.boundingBox())!;
  await page.mouse.move(box.x + 2, box.y + 100);
  await page.mouse.down();
  await page.mouse.move(box.x - 40, box.y + 100);
  await page.keyboard.press("Escape");
  await expect(panel(page)).toHaveCount(0);
  // with the panel gone, the page takes the pointer again at once
  await expect(page.locator("body")).not.toHaveClass(/is-resizing/);
  await page.mouse.up();
});

test("J and K move the focus to the row they light, and closing the panel gives it back", async ({ page, request }) => {
  await openReviewsIn(request, "panel");
  await clearInbox(request);
  await createReview(request, { title: "Focus: first" });
  await createReview(request, { title: "Focus: second" });
  await page.goto("/#/");
  await expect(page.getByTestId("review-row")).toHaveCount(2);
  await page.keyboard.press("j");
  const lit = page.locator(".review-row.is-focused");
  await expect(lit).toBeFocused();
  const title = await lit.locator(".review-row-title").textContent();
  // the panel opens from the row, and Escape closes it and returns there
  await page.keyboard.press("Enter");
  await expect(panel(page)).toBeVisible();
  await page.locator("body").press("Escape");
  await expect(panel(page)).toHaveCount(0);
  await expect(page.getByTestId("review-row").filter({ hasText: title! })).toBeFocused();
});

test("Escape pressed the moment the panel shows closes it", async ({ page, request }) => {
  await openReviewsIn(request, "panel");
  await clearInbox(request);
  await createReview(request, { title: "Quick: open and close" });
  await page.goto("/#/");
  await expect(page.getByTestId("review-row")).toHaveCount(1);
  // the Escape comes as soon as the panel is in the page, before the browser
  // has painted it: as a quick person, or a busy machine, would press it
  await page.evaluate(() => {
    const observer = new MutationObserver(() => {
      if (!document.querySelector('[data-testid="review-panel"]')) return;
      observer.disconnect();
      document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    observer.observe(document.body, { childList: true, subtree: true });
  });
  await row(page, "Quick: open and close").click();
  await expect(page).not.toHaveURL(/open=/);
  await expect(panel(page)).toHaveCount(0);
});

test("Escape pressed the moment the review shows in the panel closes it", async ({ page, request }) => {
  await openReviewsIn(request, "panel");
  await clearInbox(request);
  await createReview(request, { title: "Quick: close as it loads" });
  await page.goto("/#/");
  await expect(page.getByTestId("review-row")).toHaveCount(1);
  // the Escape comes as soon as the review is in the panel, before the
  // browser has painted it: the keys pass from the loading panel to the
  // review then, and none may be lost on the way
  await page.evaluate(() => {
    const observer = new MutationObserver(() => {
      if (!document.querySelector('[data-testid="review-panel"] [data-testid="review-title"]')) return;
      observer.disconnect();
      document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    observer.observe(document.body, { childList: true, subtree: true });
  });
  await row(page, "Quick: close as it loads").click();
  await expect(panel(page)).toHaveCount(0);
});

test("walking to another review never shows it maximized", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Panel: walk from" });
  await createReview(page.request, { title: "Panel: walk to" });
  await page.goto("/#/");
  await row(page, "Panel: walk from").click();
  await expect(page.frameLocator("#plugin-frame").locator("body")).toContainText("One proposal.");
  await page.locator("body").press("ControlOrMeta+Shift+M");
  await expect(page.locator(".frame-bar-title")).toHaveText("Panel: walk from");
  // every title the maximized bar ever shows, even for one render
  await page.evaluate(() => {
    const seen: string[] = [];
    (window as any).__maximizedTitles = seen;
    new MutationObserver((records) => {
      for (const r of records)
        for (const n of [r.target, ...r.addedNodes]) {
          const bar = (n instanceof Element ? n : n.parentElement)?.closest(".frame-bar-title");
          if (bar) seen.push(bar.textContent ?? "");
        }
    }).observe(document.body, { subtree: true, childList: true, characterData: true });
  });
  await page.locator("body").press("j");
  await expect(panel(page).getByTestId("review-title")).toHaveText("Panel: walk to");
  await expect(page.locator(".frame-bar")).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__maximizedTitles)).not.toContain("Panel: walk to");
});

test("Escape closes the panel before its review has loaded", async ({ page, request }) => {
  await openReviewsIn(request, "panel");
  await clearInbox(request);
  const review = await createReview(request, { title: "Panel: slow review" });
  await page.goto("/#/");
  // the review's own request is held until the panel has been closed
  let release: () => void = () => {};
  const held = new Promise<void>((r) => (release = r));
  await page.route(`**/api/v1/reviews/${review.id}`, async (route) => {
    await held;
    await route.continue().catch(() => {});
  });
  await row(page, "Panel: slow review").click();
  await expect(panel(page)).toBeVisible();
  await page.locator("body").press("Escape");
  await expect(panel(page)).toHaveCount(0);
  release();
});
