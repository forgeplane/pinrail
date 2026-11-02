import fs from "node:fs";
import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core, createReview } from "./helpers";

const bg = (page: Page) =>
  page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--bg").trim());

async function newTheme(request: APIRequestContext, name: string) {
  const id = `${name}-${Date.now()}`;
  const created = await request.post(`${core}/api/v1/themes`, { data: { id, from: "dark" } });
  expect(created.status(), await created.text()).toBe(201);
  const { path } = (await created.json()) as { path: string };
  return { id, path };
}

const settings = (request: APIRequestContext, appearance: Record<string, string>) =>
  request.patch(`${core}/api/v1/settings`, { data: { appearance } });

const write = (path: string, name: string, colors: Record<string, string>) =>
  fs.writeFileSync(path, JSON.stringify({ name, scheme: "dark", colors }));

test.describe.configure({ mode: "serial" });

test.afterEach(async ({ request }) => {
  await settings(request, { theme: "system", dark_theme: "dark", light_theme: "light" });
});

test("a saved theme file changes the app without a reload", async ({ page }) => {
  const { id, path } = await newTheme(page.request, "live");
  write(path, "Live", { bg: "#102030" });
  await settings(page.request, { theme: "dark", dark_theme: id });
  await page.goto("/#/history");
  await expect.poll(() => bg(page)).toBe("#102030");

  write(path, "Live", { bg: "#203040" });
  await expect.poll(() => bg(page)).toBe("#203040");

  fs.rmSync(path);
  await expect.poll(() => bg(page)).toBe("#18191b");
});

test("a custom theme paints from the cache before the server answers", async ({ page }) => {
  const { id, path } = await newTheme(page.request, "cached");
  write(path, "Cached", { bg: "#112233" });
  await settings(page.request, { theme: "dark", dark_theme: id });
  await page.goto("/#/history");
  await expect.poll(() => bg(page)).toBe("#112233");

  await page.route("**/api/v1/themes", (route) => route.abort());
  await page.reload();
  await expect.poll(() => bg(page)).toBe("#112233");
  fs.rmSync(path);
});

test("Settings chooses a theme for a scheme and shows an invalid file", async ({ page }) => {
  const good = await newTheme(page.request, "picked");
  write(good.path, "Picked", { bg: "#223344" });
  const bad = await newTheme(page.request, "broken");
  fs.writeFileSync(bad.path, "{");
  await settings(page.request, { theme: "light" });

  await page.goto("/#/history");
  await page.keyboard.press("ControlOrMeta+,");
  await page.getByRole("button", { name: "Appearance" }).click();
  await expect(page.getByText(`${bad.id}.json: The file is not valid JSON`)).toBeVisible();

  await page.getByRole("button", { name: "Dark theme", exact: true }).click();
  await page.getByRole("option", { name: "Picked" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect.poll(() => bg(page)).toBe("#223344");

  fs.rmSync(good.path);
  fs.rmSync(bad.path);
});

test("the command palette switches to a theme by its name", async ({ page }) => {
  const { id, path } = await newTheme(page.request, "found");
  const name = `Found ${id.slice(-6)}`;
  write(path, name, { bg: "#334455" });
  await settings(page.request, { theme: "dark" });

  await page.goto("/#/history");
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await palette.getByRole("textbox").fill(name);
  await palette.getByRole("option", { name: new RegExp(`Switch to the ${name} theme`) }).click();
  await expect.poll(() => bg(page)).toBe("#334455");
  fs.rmSync(path);
});

test("a plugin view takes a custom theme's colours and follows a change to them", async ({ page }) => {
  const { id, path } = await newTheme(page.request, "view");
  write(path, "View", { bg: "#445566" });
  await settings(page.request, { theme: "dark", dark_theme: id });
  const review = await createReview(page.request, { title: `themed ${id}` });

  await page.goto(`/#/reviews/${review.id}`);
  const token = () =>
    page
      .frameLocator("iframe")
      .first()
      .locator("html")
      .evaluate((root) => getComputedStyle(root).getPropertyValue("--pinrail-bg").trim());
  await expect.poll(token).toBe("#445566");

  write(path, "View", { bg: "#556677" });
  await expect.poll(token).toBe("#556677");

  await settings(page.request, { dark_theme: "dark" });
  await expect.poll(token).toBe("#18191b");
  fs.rmSync(path);
});
