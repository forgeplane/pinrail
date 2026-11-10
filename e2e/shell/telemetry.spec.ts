// The notice about usage data at the top of the inbox, and Settings ›
// Privacy: its switches and what is sent.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core, settingSaved, settingsDialog } from "./helpers";

const telemetry = (request: APIRequestContext, change: Record<string, unknown>) =>
  request.patch(`${core}/api/v1/settings`, { data: { telemetry: change } });

const notice = (page: Page) => page.getByRole("region", { name: "Usage data" });

// the notice not answered yet, as on a first launch; answered again after
test.beforeEach(async ({ request }) => {
  await telemetry(request, { notice_shown: null, usage: true, crashes: true });
});
test.afterEach(async ({ request }) => {
  await telemetry(request, { notice_shown: "2026-01-01T00:00:00Z", usage: true, crashes: true });
});

test("the inbox tells about usage data until OK is pressed, and recording starts then", async ({ page, request }) => {
  await page.goto("/#/");
  await expect(notice(page)).toContainText("anonymous usage data and crash reports");
  const before = await (await request.get(`${core}/api/v1/telemetry`)).json();
  expect(before.usage).toBe(false);

  await notice(page).getByRole("button", { name: "OK" }).click();
  await expect(notice(page)).toHaveCount(0);
  await settingSaved(request, (s) => typeof s.telemetry.notice_shown, "string");
  const after = await (await request.get(`${core}/api/v1/telemetry`)).json();
  expect(after.usage).toBe(true);
  expect(after.waiting.map((e: { name: string }) => e.name)).toContain("app_started");

  // it stays answered
  await page.reload();
  await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();
  await expect(notice(page)).toHaveCount(0);
});

test("Settings on the notice opens the Privacy page, whose switches turn usage data and crash reports off", async ({
  page,
  request,
}) => {
  await page.goto("/#/");
  await notice(page).getByRole("button", { name: "Settings" }).click();
  await expect(settingsDialog(page).getByRole("heading", { name: "Privacy" })).toBeVisible();
  await expect(notice(page)).toHaveCount(0);

  const usage = settingsDialog(page).getByRole("switch", { name: "Send usage data" });
  const crashes = settingsDialog(page).getByRole("switch", { name: "Send crash reports" });
  await expect(usage).toBeChecked();
  await usage.click();
  await expect(usage).not.toBeChecked();
  await settingSaved(request, (s) => s.telemetry.usage, false);
  await crashes.click();
  await settingSaved(request, (s) => s.telemetry.crashes, false);
  // nothing waits and no install id once usage data is off
  const status = await (await request.get(`${core}/api/v1/telemetry`)).json();
  expect(status.waiting).toEqual([]);
  expect(status.install).toBeNull();
});

test("Show what is sent lists the waiting events as JSON", async ({ page, request }) => {
  await telemetry(request, { notice_shown: "2026-11-10T09:00:00Z" });
  await request.post(`${core}/api/v1/telemetry/features`, { data: { feature: "page_projects" } });
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await settingsDialog(page)
    .getByRole("navigation", { name: "Settings sections" })
    .getByRole("button", { name: "Privacy", exact: true })
    .click();
  const show = settingsDialog(page).getByRole("button", { name: "Show what is sent" });
  await show.click();
  const sent = settingsDialog(page).getByTestId("telemetry-sent");
  await expect(sent).toContainText('"feature": "page_projects"');
  await expect(sent).toContainText('"install"');
  await settingsDialog(page).getByRole("button", { name: "Hide" }).click();
  await expect(sent).toHaveCount(0);
});

test("an uncaught error of the window waits as a crash report", async ({ page, request }) => {
  // none waiting, with crash reports on: turning them off deletes one
  await telemetry(request, { notice_shown: "2026-11-10T09:00:00Z", crashes: false });
  await telemetry(request, { crashes: true });
  await page.goto("/#/");
  await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();
  await page.evaluate(() => {
    setTimeout(() => {
      throw new Error('Could not open "Acme payroll" from /Users/ana/acme');
    });
  });
  await expect
    .poll(async () => (await (await request.get(`${core}/api/v1/telemetry`)).json()).crash?.message)
    .toBe('Could not open "…" from acme');
  const crash = (await (await request.get(`${core}/api/v1/telemetry`)).json()).crash;
  expect(crash.kind).toBe("ui");
});
