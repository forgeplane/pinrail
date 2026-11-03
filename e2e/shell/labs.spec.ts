// Sessions and questions are in Settings › Labs, off until turned on. While
// they are off, the sidebar lists what waits and the projects, and nothing
// about sessions shows; the rest of the suite runs with them on.

import { expect, test, type APIRequestContext } from "@playwright/test";
import { clearInbox, core, createReview, settingsSection } from "./helpers";

const sessions = (request: APIRequestContext, on: boolean) =>
  request.patch(`${core}/api/v1/settings`, { data: { labs: { sessions: on } } });

test.afterEach(async ({ request }) => {
  await sessions(request, true);
});

test("with sessions off the sidebar lists what waits and the projects, and nothing about sessions", async ({
  page,
}) => {
  await sessions(page.request, false);
  await clearInbox(page.request);
  await createReview(page.request, { title: "Labs: in api", origin: { repo: "acme/api", workflow: "w" } });
  await createReview(page.request, { title: "Labs: in web", origin: { repo: "acme/web", workflow: "w" } });
  await page.goto("/#/");

  const sidebar = page.getByRole("complementary", { name: "Workspace" });
  const waiting = sidebar.getByRole("region", { name: "Waiting" });
  await expect(waiting.getByTestId("waiting-review")).toHaveText(["Labs: in api", "Labs: in web"]);
  const projects = sidebar.getByRole("region", { name: "Projects" });
  await expect(projects.getByRole("link", { name: /acme\/api/ })).toContainText("1");
  await projects.getByRole("link", { name: /acme\/web/ }).click();
  await expect(page).toHaveURL(/repo=acme%2Fweb/);

  await expect(sidebar.getByRole("link", { name: "Sessions" })).toHaveCount(0);
  await expect(sidebar.getByRole("region", { name: "Repositories" })).toHaveCount(0);
  await expect(sidebar.getByRole("button", { name: /Search/ })).toBeVisible();

  // the Sessions page leads to the inbox
  await page.goto("/#/sessions");
  await expect(page).toHaveURL(/#\/$/);

  // the search has no stages
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await expect(palette.getByRole("tab", { name: "Inbox" })).toBeVisible();
  await expect(palette.getByRole("tab", { name: "Stages" })).toHaveCount(0);
});

test("turning sessions on in Settings › Labs brings them back, and the core keeps it", async ({ page }) => {
  await sessions(page.request, false);
  await page.goto("/#/");
  const sidebar = page.getByRole("complementary", { name: "Workspace" });
  await expect(sidebar.getByRole("link", { name: "Sessions" })).toHaveCount(0);

  await page.keyboard.press("ControlOrMeta+,");
  await settingsSection(page, "Labs").click();
  const toggle = page.getByRole("switch", { name: "Sessions and questions" });
  await expect(toggle).not.toBeChecked();
  await toggle.click();
  await expect(toggle).toBeChecked();
  await page.keyboard.press("Escape");

  await expect(sidebar.getByRole("link", { name: "Sessions" })).toBeVisible();
  await expect(sidebar.getByRole("region", { name: "Repositories" })).toBeVisible();
  await expect(sidebar.getByRole("region", { name: "Projects" })).toHaveCount(0);
  const saved = await (await page.request.get(`${core}/api/v1/settings`)).json();
  expect(saved.labs.sessions).toBe(true);
});
