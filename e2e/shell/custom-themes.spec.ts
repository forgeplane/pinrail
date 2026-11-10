import fs from "node:fs";
import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core, createReview } from "./helpers";

// a route handler may still be reading a response when the test ends, and
// closing the page disposes it: such a handler's error is not the test's
test.afterEach(({ page }) => page.unrouteAll({ behavior: "ignoreErrors" }));

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

/** Records, for a test that fails now and then, what the page heard about
 *  the themes: each event from the core, and each request for the themes
 *  with its answer. `attach` saves them in the test's results, with the
 *  themes the core lists at that moment. */
async function recordThemes(page: Page) {
  await page.addInitScript(() => {
    const seen: unknown[] = [];
    (window as unknown as { __themeEvents: unknown[] }).__themeEvents = seen;
    const Original = window.EventSource;
    window.EventSource = class extends Original {
      constructor(url: string | URL, init?: EventSourceInit) {
        super(url, init);
        const at = () => Math.round(performance.now());
        seen.push({ at: at(), what: "connect", url: String(url) });
        this.addEventListener("open", () => seen.push({ at: at(), what: "open" }));
        this.addEventListener("error", () => seen.push({ at: at(), what: "error" }));
        this.addEventListener("themes_changed", (e) =>
          seen.push({ at: at(), what: "themes_changed", id: (e as MessageEvent).lastEventId }),
        );
      }
    };
  });
  const requests: unknown[] = [];
  const start = Date.now();
  page.on("response", async (response) => {
    if (!new URL(response.url()).pathname.endsWith("/api/v1/themes")) return;
    const body = await response.json().catch(() => null);
    const ids = (body as { themes?: { id: string }[] } | null)?.themes?.map((t) => t.id);
    requests.push({ at: Date.now() - start, status: response.status(), ids });
  });
  return async () => {
    const events = await page
      .evaluate(() => (window as unknown as { __themeEvents: unknown[] }).__themeEvents)
      .catch(() => null);
    const listed = await (await page.request.get(`${core}/api/v1/themes`)).json().catch(() => null);
    const ids = (listed as { themes?: { id: string }[] } | null)?.themes?.map((t) => t.id);
    // a file beside the trace, which the list reporter names
    const file = test.info().outputPath("theme-diagnostics.json");
    fs.writeFileSync(file, JSON.stringify({ events, requests, listedNow: ids }, null, 2));
    await test.info().attach("theme-diagnostics", { path: file, contentType: "application/json" });
  };
}

// It failed once in many full runs: the colours of a deleted theme stayed.
// On a failure, the report has what the page heard, to tell whether the
// core announced the change, the page asked again, or the answer still
// listed the theme.
test("a saved theme file changes the app without a reload", async ({ page }) => {
  const attach = await recordThemes(page);
  const { id, path } = await newTheme(page.request, "live");
  try {
    write(path, "Live", { bg: "#102030" });
    await settings(page.request, { theme: "dark", dark_theme: id });
    await page.goto("/#/history");
    await expect.poll(() => bg(page)).toBe("#102030");

    write(path, "Live", { bg: "#203040" });
    await expect.poll(() => bg(page)).toBe("#203040");

    fs.rmSync(path);
    await expect.poll(() => bg(page)).toBe("#18191b");
  } catch (error) {
    await attach();
    throw error;
  }
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

test("an answer about the themes that arrives late does not undo a newer one", async ({ page }) => {
  const { id, path } = await newTheme(page.request, "late");
  write(path, "Late", { bg: "#102030" });
  await settings(page.request, { theme: "dark", dark_theme: id });
  await page.goto("/#/history");
  await expect.poll(() => bg(page)).toBe("#102030");

  // the next answer is read at once and held back, so that a later one
  // passes it
  let held: () => void = () => {};
  const first = new Promise<void>((seen) => {
    let calls = 0;
    void page.route("**/api/v1/themes", async (route) => {
      if (++calls > 1) return route.continue();
      const answer = await route.fetch();
      seen();
      await new Promise<void>((release) => (held = release));
      await route.fulfill({ response: answer });
    });
  });
  write(path, "Late", { bg: "#203040" });
  await first;
  write(path, "Late", { bg: "#304050" });
  await expect.poll(() => bg(page)).toBe("#304050");
  held();
  await page.waitForTimeout(500);
  expect(await bg(page)).toBe("#304050");
});
