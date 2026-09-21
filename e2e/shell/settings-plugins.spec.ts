import { expect, test, type Page } from "@playwright/test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const root = path.resolve(__dirname, "..", "..");
const core = "http://127.0.0.1:4799";

/** A copy of a sample plugin under a name and version of its own. */
function pluginCopy(sample: string, name: string, version: string, extra: Record<string, unknown> = {}): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), `wicket-${name}-`));
  const from = path.join(root, "plugins", sample);
  // the plugin as a bundle: the folder without its tests and fixtures
  fs.cpSync(from, dir, { recursive: true, filter: (src) => !/\/(tests|fixtures|node_modules)(\/|$)/.test(src) });
  const manifest = JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8"));
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ ...manifest, name, version, ...extra }));
  return dir;
}

async function openInstall(page: Page) {
  await page.goto("/#/plugins");
  await expect(page.locator("[data-settings]")).toBeVisible();
  await page.locator("[data-install-open]").click();
  const panel = page.locator("[data-install-panel]");
  await expect(panel).toBeVisible();
  return panel;
}

test("the Plugins section lists what is installed and has no directories", async ({ page }) => {
  await page.goto("/#/plugins");
  await expect(page.locator("[data-settings]")).toBeVisible();
  await expect(page.locator('[data-plugin-row="list"]')).toBeVisible();
  await expect(page.locator(".settings-group h3", { hasText: "Directories" })).toHaveCount(0);
  await expect(page.locator("[data-install-open]")).toBeEnabled();
});

test("a folder is looked at before it is installed, and its row says where it came from", async ({ page }) => {
  const source = pluginCopy("hello", "greeter", "1.2.0");
  const dialog = await openInstall(page);
  await dialog.getByLabel("Source").fill(source);
  await dialog.locator("[data-install-look]").click();

  const seen = dialog.locator("[data-install-seen]");
  await expect(seen).toContainText("greeter · 1.2.0");
  await expect(seen).toContainText(`From the folder ${source}`);
  await expect(seen.locator('[data-runs="nothing"]')).toContainText("No build");
  await expect(seen.locator("[data-replaces]")).toHaveCount(0);

  await dialog.locator("[data-install-confirm]").click();
  await expect(dialog.locator("[data-install-done]")).toContainText("1.2.0 is ready");
  await dialog.locator("[data-install-close]").click();

  const row = page.locator('[data-plugin-row="greeter"]');
  await expect(row).toContainText("ready");
  await expect(row).toContainText(`copied from ${source}`);

  // the copy is what is served: a record with a hash, not a link
  const plugins = await (await page.request.get(`${core}/api/v1/plugins`)).json();
  const greeter = plugins.plugins.find((p: { name: string }) => p.name === "greeter");
  expect(greeter.install.kind).toBe("path");
  expect(greeter.install.linked).toBe(false);
  expect(greeter.install.hash).toBeTruthy();

  // checking for updates of a copy compares the folder with the store
  await row.getByRole("button", { name: "Check for updates of greeter" }).click();
  await expect(row.locator("[data-plugin-updates]")).toHaveText("Up to date");

  // once the folder changes, the check offers an update, which copies it again
  fs.writeFileSync(path.join(source, "view/index.html"), "<html>second</html>");
  await row.getByRole("button", { name: "Check for updates of greeter" }).click();
  await expect(row.locator("[data-plugin-updates]")).toHaveText("The folder changed since it was copied");
  await row.locator("[data-plugin-update]").click();
  await expect(row.locator("[data-plugin-updates]")).toHaveText("Updated to 1.2.0");
  const served = await (await page.request.get(`${core}/plugins/greeter/1/view/index.html`)).text();
  expect(served).toBe("<html>second</html>");

  // the same version again says what it replaces
  const again = await openInstall(page);
  await again.getByLabel("Source").fill(source);
  await again.locator("[data-install-look]").click();
  await expect(again.locator('[data-replaces="unchanged"]')).toContainText("greeter 1.2.0 is installed already, from this source, and nothing has changed");
  // once the folder changes, the same version replaces what is there
  fs.appendFileSync(path.join(source, "view/index.html"), "<!-- edited -->");
  await again.locator("[data-install-look]").click();
  await expect(again.locator('[data-replaces="same"]')).toContainText("greeter 1.2.0 is installed already. Installing replaces it.");
  await again.getByLabel("Source").press("Escape");

  // removing asks once, in the row, then the row goes
  await row.getByRole("button", { name: "Remove greeter" }).click();
  await expect(row.locator("[data-plugin-remove-ask]")).toContainText("Remove Hello?");
  await row.locator("[data-plugin-remove-confirm]").click();
  await expect(page.locator('[data-plugin-row="greeter"]')).toHaveCount(0);
  // the app says so in the corner, and the line can be dismissed
  const removed = page.locator("[data-toast]").filter({ hasText: "Hello plugin was removed" });
  await expect(removed).toHaveCount(1);
  await removed.locator(".toast-close").click();
  await expect(removed).toHaveCount(0);
  const after = await (await page.request.get(`${core}/api/v1/plugins`)).json();
  expect(after.plugins.some((p: { name: string }) => p.name === "greeter")).toBe(false);
});

test("a source that builds shows the exact command as the consent, then runs it", async ({ page }) => {
  const source = pluginCopy("hello", "compiled", "1.0.0", {
    build: { command: "echo building the view && mkdir -p view && printf '<html>built</html>' > view/index.html" },
  });
  fs.rmSync(path.join(source, "view/index.html"));
  const dialog = await openInstall(page);
  await dialog.getByLabel("Source").fill(source);
  await dialog.locator("[data-install-look]").click();

  const runs = dialog.locator('[data-runs="build"]');
  await expect(runs).toContainText("echo building the view && mkdir -p view && printf '<html>built</html>' > view/index.html");
  await expect(runs).toContainText("with your rights");

  await dialog.locator("[data-install-confirm]").click();
  await expect(dialog.locator("[data-install-done]")).toContainText("1.0.0 is ready");
  await dialog.locator("[data-install-close]").click();
  await expect(page.locator('[data-plugin-row="compiled"]')).toContainText("ready");

  const bundle = await (await page.request.get(`${core}/plugins/compiled/1/view/index.html`)).text();
  expect(bundle).toBe("<html>built</html>");
});

test("a link serves the folder live and offers to install a copy", async ({ page }) => {
  const source = pluginCopy("hello", "wip", "1.0.0");
  const dialog = await openInstall(page);
  await dialog.getByLabel("Source").fill(source);
  await dialog.getByRole("switch", { name: "Link instead of copying" }).click();
  await dialog.locator("[data-install-look]").click();
  await expect(dialog.locator('[data-runs="nothing"]')).toContainText("Nothing is copied");
  await expect(dialog.locator("[data-install-confirm]")).toHaveText("Link");
  await dialog.locator("[data-install-confirm]").click();
  await expect(dialog.locator("[data-install-done]")).toBeVisible();
  await dialog.locator("[data-install-close]").click();

  const row = page.locator('[data-plugin-row="wip"]');
  await expect(row).toContainText("linked");
  await expect(row).toContainText(`linked · ${source}`);

  // a copy from the row: the dialog opens looked at already, and says it replaces the link
  await row.getByRole("button", { name: "Install a copy of wip" }).click();
  const copy = page.locator("[data-install-panel]");
  await expect(copy.locator('[data-replaces="link"]')).toContainText("wip 1.0.0 is installed already, as a link to this very folder");
  await copy.locator("[data-install-confirm]").click();
  await expect(copy.locator("[data-install-done]")).toBeVisible();
  await copy.locator("[data-install-close]").click();
  await expect(row).toContainText(`copied from ${source}`);
});

test("what is not a plugin is refused before anything runs", async ({ page }) => {
  const empty = fs.mkdtempSync(path.join(os.tmpdir(), "wicket-empty-"));
  const dialog = await openInstall(page);
  await dialog.getByLabel("Source").fill(empty);
  await dialog.locator("[data-install-look]").click();
  await expect(dialog.locator(".install-error")).toContainText("manifest.json");
  await expect(dialog.locator("[data-install-confirm]")).toHaveCount(0);

  // Esc in the field closes the panel, not the settings around it
  await dialog.getByLabel("Source").press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(page.locator("[data-settings]")).toBeVisible();
  await expect(page.locator("[data-install-open]")).toBeVisible();
});
