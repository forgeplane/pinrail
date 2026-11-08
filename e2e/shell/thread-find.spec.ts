// Finding text in a session's thread with ⌘F, as a browser's find in page
// does, while a review open beside the thread keeps its own find.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core, onePayload, openReviewsIn } from "./helpers";

let n = 0;
const fresh = () => `s-find-${Date.now()}-${++n}`;

async function note(request: APIRequestContext, session: string, text: string) {
  const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { text, kind: "note" },
  });
  expect(response.status(), await response.text()).toBe(200);
}

const bar = (page: Page) => page.getByRole("search", { name: "Find in the thread" });
const field = (page: Page) => bar(page).getByRole("textbox", { name: "Find in the thread" });
const highlighted = (page: Page, name: string) => page.evaluate((name) => CSS.highlights.get(name)?.size ?? 0, name);

test.afterEach(({ request }) => openReviewsIn(request, "panel"));

test("⌘F finds text in the thread, steps through the matches and closes with Escape", async ({ page, request }) => {
  const session = fresh();
  await note(request, session, "The zephyr loader reads each theme file.");
  for (let i = 0; i < 12; i++) await note(request, session, `Step ${i}: nothing to see here.`);
  await note(request, session, "Checked the Zephyr schema.");
  await note(request, session, "A zephyr theme and another zephyr theme.");
  await page.setViewportSize({ width: 1300, height: 700 });
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(page.getByText("A zephyr theme and another")).toBeVisible();

  await page.keyboard.press("ControlOrMeta+f");
  await expect(field(page)).toBeFocused();
  await field(page).fill("zephyr");
  const count = bar(page).getByRole("status");
  // the first match from the top of what shows
  await expect(count).toHaveText(/^\d of 4$/);
  expect(await highlighted(page, "pinrail-find")).toBe(4);
  // WebKit paints a highlight only on text that can be selected, and puts
  // it on other text when it cannot
  const unselectable = await page.evaluate(
    () =>
      [...(CSS.highlights.get("pinrail-find") as Iterable<Range>)].filter((range) => {
        const style = getComputedStyle(range.startContainer.parentElement!);
        return (style.userSelect || style.webkitUserSelect) === "none";
      }).length,
  );
  expect(unselectable).toBe(0);

  await field(page).press("Enter");
  await field(page).press("Enter");
  await field(page).press("Enter");
  // round to the first, at the top of the thread, which is scrolled to
  await expect(page.getByText("The zephyr loader")).toBeInViewport();
  const first = await count.textContent();
  await field(page).press("Shift+Enter");
  await expect(count).not.toHaveText(first!);
  await field(page).press("Enter");
  await expect(count).toHaveText(first!);

  await field(page).fill("quokka");
  await expect(count).toHaveText("No matches");

  await field(page).press("Escape");
  await expect(bar(page)).toHaveCount(0);
  expect(await highlighted(page, "pinrail-find")).toBe(0);
  // the search comes back as the bar opens again
  await page.keyboard.press("ControlOrMeta+f");
  await expect(field(page)).toHaveValue("quokka");
});

test("with the focus in a review beside the thread, ⌘F finds in the review", async ({ page, request }) => {
  await openReviewsIn(request, "panel");
  const session = fresh();
  await note(request, session, "Before the review.");
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "list",
      title: "Find target",
      origin: { repo: "acme/api" },
      requested_by: "claude-code",
      session,
      payload: onePayload,
    },
  });
  expect(response.status(), await response.text()).toBe(201);
  await page.goto(`/#/sessions/claude-code/${session}`);
  await page.getByTestId("thread-review").click();
  const panel = page.getByTestId("review-panel");
  await expect(page.frameLocator("#plugin-frame").locator("body")).toContainText("One proposal.");
  await panel.locator("#plugin-frame").focus();
  await page.keyboard.press("ControlOrMeta+f");
  await expect(panel.getByRole("search", { name: "Find in the view" })).toBeVisible();
  await expect(bar(page)).toHaveCount(0);

  // from the thread, the thread's bar opens
  await page.getByText("Before the review.").click();
  await page.keyboard.press("ControlOrMeta+f");
  await expect(field(page)).toBeFocused();
});
