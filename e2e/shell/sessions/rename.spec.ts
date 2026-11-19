// A session's name in its header, renamed in place.

import { expect, test, type Page } from "../fixtures";
import { core, sessionName } from "../helpers";

let n = 0;
const fresh = () => `s-rename-${Date.now()}-${++n}`;

/** A session that started a stage, and whose hooks say it works. */
async function started(page: Page, session: string, text: string) {
  const base = `${core}/api/v1/sessions/claude-code/${session}`;
  const response = await page.request.post(`${base}/status`, {
    data: { kind: "start", text, origin: { repo: "acme/api" } },
  });
  expect(response.status(), await response.text()).toBe(200);
  const reported = await page.request.post(`${base}/reports`, {
    data: { event: "working", at: new Date().toISOString() },
  });
  expect(reported.status(), await reported.text()).toBe(204);
}

const thread = (page: Page) => page.getByTestId("thread");
const heading = (page: Page) => thread(page).getByRole("heading", { level: 1 });
const field = (page: Page) => thread(page).getByRole("textbox", { name: "Session name" });

test("a click on the session's name renames it in place, Enter or a click elsewhere saves, Esc cancels", async ({
  page,
}) => {
  const session = fresh();
  await started(page, session, "Fix the login redirect");
  const generated = await sessionName(page.request, "claude-code", session);
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(heading(page)).toHaveText("Fix the login redirect");

  // the field starts with the name shown, selected, and what follows the
  // name stays where it was
  const chip = thread(page).getByTestId("presence-chip");
  const before = (await chip.boundingBox())!.x;
  await heading(page).getByRole("button").click();
  expect(Math.abs((await chip.boundingBox())!.x - before)).toBeLessThan(1);
  await expect(field(page)).toBeFocused();
  await expect(field(page)).toHaveValue("Fix the login redirect");
  // the new name shows at once, before the core has answered
  let answer = () => {};
  const answered = new Promise<void>((resolve) => (answer = resolve));
  await page.route(new RegExp(`/api/v1/sessions/claude-code/${session}$`), async (route) => {
    if (route.request().method() === "PATCH") await answered;
    await route.continue();
  });
  await page.keyboard.type("Login work");
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Login work");
  answer();
  await expect(heading(page)).toHaveText("Login work");
  await expect(thread(page).locator(".thread-meta-stage")).toHaveText("Fix the login redirect");

  // Esc keeps the name
  await heading(page).getByRole("button").click();
  await page.keyboard.type("Not this");
  await page.keyboard.press("Escape");
  await expect(heading(page)).toHaveText("Login work");

  // a click elsewhere saves
  await heading(page).getByRole("button").click();
  await page.keyboard.type("Auth fixes");
  await thread(page).locator(".thread-items").click();
  await expect(heading(page)).toHaveText("Auth fixes");

  // an empty name gives back the stage, with the name Pinrail gave it beside it
  await heading(page).getByRole("button").click();
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Fix the login redirect");
  await expect(thread(page).locator(".thread-meta-generated")).toHaveText(generated);
});

test("Rename in a session's menu in the sidebar opens it to rename", async ({ page }) => {
  const session = fresh();
  await started(page, session, "Sort the menu");
  await page.goto("/#/");
  const row = page.locator(`[data-testid="sidebar-session"][href="#/sessions/claude-code/${session}"]`);
  await row.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Rename" }).click();
  await expect(page).toHaveURL(new RegExp(`/sessions/claude-code/${session}$`));
  await expect(field(page)).toBeFocused();
  await page.keyboard.type("Menu work");
  await page.keyboard.press("Enter");
  await expect(heading(page)).toHaveText("Menu work");
  await expect(row).toContainText("Menu work");
});
