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
  for (const entry of fs.readdirSync(from, { withFileTypes: true })) {
    if (entry.isFile()) fs.copyFileSync(path.join(from, entry.name), path.join(dir, entry.name));
  }
  const manifest = JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8"));
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ ...manifest, name, version, ...extra }));
  return dir;
}

async function openInstall(page: Page) {
  await page.goto("/#/plugins");
  await expect(page.locator("[data-settings]")).toBeVisible();
  await page.locator("[data-install-open]").click();
  const dialog = page.locator("[data-install-dialog]");
  await expect(dialog).toBeVisible();
  return dialog;
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

  // the same version again says what it replaces
  const again = await openInstall(page);
  await again.getByLabel("Source").fill(source);
  await again.locator("[data-install-look]").click();
  await expect(again.locator('[data-replaces="same"]')).toContainText("Replaces greeter 1.2.0");
});

test("a source that builds shows the exact command as the consent, then runs it", async ({ page }) => {
  const source = pluginCopy("hello", "compiled", "1.0.0", {
    build: { command: "echo building the view && printf '<html>built</html>' > index.html" },
  });
  fs.rmSync(path.join(source, "index.html"));
  const dialog = await openInstall(page);
  await dialog.getByLabel("Source").fill(source);
  await dialog.locator("[data-install-look]").click();

  const runs = dialog.locator('[data-runs="build"]');
  await expect(runs).toContainText("echo building the view && printf '<html>built</html>' > index.html");
  await expect(runs).toContainText("with your rights");

  await dialog.locator("[data-install-confirm]").click();
  await expect(dialog.locator("[data-install-done]")).toContainText("1.0.0 is ready");
  await dialog.locator("[data-install-close]").click();
  await expect(page.locator('[data-plugin-row="compiled"]')).toContainText("ready");

  const bundle = await (await page.request.get(`${core}/plugins/compiled/1/index.html`)).text();
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
  const copy = page.locator("[data-install-dialog]");
  await expect(copy.locator('[data-replaces="link"]')).toContainText("Replaces the link to wip 1.0.0");
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

  // Esc closes the dialog, not the settings behind it
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(page.locator("[data-settings]")).toBeVisible();
});
