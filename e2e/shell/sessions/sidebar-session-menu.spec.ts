// A session's menu in the sidebar: a ⋯ button on its row, shown on hover,
// or a right-click on the row.

import { expect, test, type APIRequestContext } from "../fixtures";
import { core, sessionId } from "../helpers";

let n = 0;
const fresh = () => `s-menu-${Date.now()}-${++n}`;

async function post(request: APIRequestContext, session: string, path: string, data: unknown) {
  const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/${path}`, { data });
  expect(response.status(), await response.text()).toBeLessThan(300);
}

test("a session's menu marks it read and unread, and archives it", async ({ page }) => {
  const session = fresh();
  await post(page.request, session, "status", {
    kind: "start",
    text: "Fix the login redirect",
    origin: { repo: "acme/api" },
  });
  await post(page.request, session, "reports", { event: "prompt", detail: { prompt: "Fix it" } });
  await post(page.request, session, "reports", { event: "turn_ended", end: "done", detail: { reply: "Fixed." } });
  await page.goto("/#/");
  const id = await sessionId(page.request, "claude-code", session);
  const row = page.locator(`[data-testid="sidebar-session"][href$="/${id}"]`);
  await expect(row).toHaveAttribute("data-state", "finished");
  const button = page.locator(".sidebar-session-item", { has: row }).getByTestId("sidebar-session-menu");

  // the button shows on hover
  await expect(button).toHaveCSS("opacity", "0");
  await row.hover();
  await expect(button).toHaveCSS("opacity", "1");
  await button.click();
  const menu = page.getByRole("menu");
  await expect(menu.getByRole("menuitem")).toHaveText([
    "Rename",
    "Archive",
    "Mark as read",
    "Copy session id",
    "Delete…",
  ]);
  await menu.getByRole("menuitem", { name: "Mark as read" }).click();
  await expect(row).toHaveAttribute("data-state", "idle");

  // a right-click opens it too
  await row.click({ button: "right" });
  await menu.getByRole("menuitem", { name: "Mark as unread" }).click();
  await expect(row).toHaveAttribute("data-state", "finished");

  await row.click({ button: "right" });
  await menu.getByRole("menuitem", { name: "Archive" }).click();
  await expect(row).toHaveCount(0);
});
