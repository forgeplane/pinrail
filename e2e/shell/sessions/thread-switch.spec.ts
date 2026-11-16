// A session opened lately shows at once when opened again, before it has
// loaded again.

import { expect, test } from "../fixtures";
import { core } from "../helpers";

let n = 0;
const fresh = () => `s-switch-${Date.now()}-${++n}`;

test("a session opened again shows its thread before it loads again, then what changed", async ({ page, request }) => {
  const [first, second] = [fresh(), fresh()];
  const report = async (session: string, text: string) => {
    const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, { data: { text } });
    expect(response.status(), await response.text()).toBe(200);
  };
  await report(first, "The first session's step");
  await report(second, "The second session's step");
  await page.goto(`/#/sessions/claude-code/${first}`);
  await expect(page.getByText("The first session's step")).toBeVisible();
  await page.goto(`/#/sessions/claude-code/${second}`);
  await expect(page.getByText("The second session's step")).toBeVisible();

  // the first session's load waits until it is let go
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route(new RegExp(`/api/v1/sessions/claude-code/${first}$`), async (route) => {
    await held;
    await route.continue();
  });
  await report(first, "A step while it was away");
  await page.goto(`/#/sessions/claude-code/${first}`);
  await expect(page.getByText("The first session's step")).toBeVisible();
  await expect(page.getByText("A step while it was away")).toHaveCount(0);
  release();
  await expect(page.getByText("A step while it was away")).toBeVisible();
});

test("the breadcrumb names the session shown, whatever ends loading last", async ({ page, request }) => {
  const [first, second] = [fresh(), fresh()];
  for (const [session, text] of [
    [first, "First task"],
    [second, "Second task"],
  ]) {
    const response = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
      data: { kind: "start", text },
    });
    expect(response.status(), await response.text()).toBe(200);
  }
  const crumb = page.locator(".crumb-title");
  await page.goto(`/#/sessions/claude-code/${first}`);
  await expect(crumb).toHaveText("First task");

  // the second session's load ends only after the first is shown again
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route(new RegExp(`/api/v1/sessions/claude-code/${second}$`), async (route) => {
    await held;
    await route.continue();
  });
  await page.goto(`/#/sessions/claude-code/${second}`);
  await page.goto(`/#/sessions/claude-code/${first}`);
  await expect(crumb).toHaveText("First task");
  release();
  await page.waitForTimeout(500);
  await expect(crumb).toHaveText("First task");
});
