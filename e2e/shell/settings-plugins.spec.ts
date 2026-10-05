import { expect, test, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { core, linkPlugin } from "./helpers";
import { scratch } from "../helpers/scratch";

const root = path.resolve(__dirname, "..", "..");

/** A copy of a sample plugin under a name and version of its own. */
function pluginCopy(sample: string, name: string, version: string, extra: Record<string, unknown> = {}): string {
  const dir = scratch(`pinrail-${name}-`);
  const from = path.join(root, "plugins", sample);
  // the plugin as a bundle: the folder without its tests and fixtures
  fs.cpSync(from, dir, { recursive: true, filter: (src) => !/\/(tests|fixtures|node_modules)(\/|$)/.test(src) });
  const manifest = JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8"));
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ ...manifest, name, version, ...extra }));
  return dir;
}

/** The page a plugin's new reviews show: its current bundle's view, fetched. */
async function servedView(page: Page, name: string): Promise<string> {
  const { plugins } = await (await page.request.get(`${core}/api/v1/plugins`)).json();
  const plugin = plugins.find((p: { name: string }) => p.name === name);
  return (await page.request.get(`${core}/bundles/${plugin.install.bundle}/view/index.html`)).text();
}

async function openInstall(page: Page) {
  await page.goto("/#/plugins");
  await expect(page.locator("[data-settings]")).toBeVisible();
  const panel = page.locator("[data-install-panel]");
  await expect(panel).toBeVisible();
  return panel;
}

test("the Plugins section lists what is installed and has no directories", async ({ page }) => {
  await page.goto("/#/plugins");
  await expect(page.locator("[data-settings]")).toBeVisible();
  await expect(page.locator('[data-plugin-row="list"]')).toBeVisible();
  await expect(page.locator(".settings-group h3", { hasText: "Directories" })).toHaveCount(0);
  // one field, there at once, for an official plugin's name or a path
  const source = page.locator("[data-install-panel]").getByLabel("Source");
  await expect(source).toBeEnabled();
  await expect(source).toHaveAttribute(
    "placeholder",
    "Search official plugins, or paste the path of a folder or a zip",
  );
});

test("the official plugins not installed are listed, found by name, and installed in one click", async ({ page }) => {
  // feedback removed, as a person who never chose it has it
  expect((await page.request.delete(`${core}/api/v1/plugins/feedback`)).status()).toBe(200);
  const panel = await openInstall(page);
  const official = panel.locator("[data-official-plugins]");
  await expect(official).toContainText("Official plugins");
  await expect(official.locator('[data-official="feedback"]')).toBeVisible();
  await expect(official.locator('[data-official="list"]')).toHaveCount(0);

  // words search them all, the installed ones said to be installed
  const field = panel.getByLabel("Source");
  await field.fill("action");
  await expect(official.locator('[data-official="list"] [data-official-installed]')).toHaveText("Installed");
  await expect(official.locator('[data-official="feedback"]')).toHaveCount(0);
  await field.fill("nothing like it");
  await expect(panel.locator("[data-official-none]")).toContainText("No official plugin matches “nothing like it”");
  await field.fill("");

  await official.locator('[data-official-install="feedback"]').click();
  const row = page.locator('[data-plugin-row="feedback"]');
  await expect(row).toBeVisible();
  await row.locator(".settings-plugin-toggle").click();
  await expect(row).toContainText("Installed from Pinrail's plugins as forgeplane/feedback");
  // installed, it is no longer offered
  await expect(panel.locator('[data-official="feedback"]')).toHaveCount(0);
});

test("an official plugin is offered the newer version the app carries, and updated from its row", async ({ page }) => {
  // notes is offered at 1.0.0 and 1.1.0; 1.0.0 is installed
  const installed = await page.request.post(`${core}/api/v1/plugins/install`, {
    data: { id: "forgeplane/notes", version: "1.0.0" },
  });
  expect(installed.status(), await installed.text()).toBe(200);
  await page.goto("/#/plugins");
  const row = page.locator('[data-plugin-row="notes"]');
  await expect(row).toContainText("1.0.0");
  const update = row.locator('[data-plugin-update="notes"]');
  await expect(update).toHaveText("Update to 1.1.0");

  // the search says so too
  const panel = page.locator("[data-install-panel]");
  await panel.getByLabel("Source").fill("notes");
  await expect(panel.locator('[data-official-install="notes"]')).toHaveText("Update to 1.1.0");

  await update.click();
  await expect(row).toContainText("1.1.0");
  await expect(update).toHaveCount(0);
  await expect(panel.locator('[data-official="notes"] [data-official-installed]')).toHaveText("Installed");
  expect((await page.request.delete(`${core}/api/v1/plugins/notes`)).status()).toBe(200);
});

test("a folder is looked at before it is installed, and its row says where it came from", async ({ page }) => {
  const source = pluginCopy("hello", "greeter", "1.2.0");
  const dialog = await openInstall(page);
  // a pause in typing is enough: the folder is looked at without a click
  await dialog.getByLabel("Source").fill(source);

  const seen = dialog.locator("[data-install-seen]");
  await expect(seen).toContainText("greeter · 1.2.0");
  await expect(seen).toContainText(`From the folder ${source}`);
  await expect(seen.locator('[data-runs="nothing"]')).toContainText("Nothing runs on your computer");
  await expect(seen.locator("[data-replaces]")).toHaveCount(0);

  await expect(dialog.locator("[data-install-confirm]")).toHaveText("Install");
  await dialog.locator("[data-install-confirm]").click();

  // the app says so, the field is ready for the next one, and the row is marked
  await expect(page.locator("[data-toast]").filter({ hasText: "Hello 1.2.0 is installed" })).toHaveCount(1);
  await expect(dialog.getByLabel("Source")).toHaveValue("");
  await expect(seen).toHaveCount(0);
  const row = page.locator('[data-plugin-row="greeter"]');
  await expect(row).toHaveClass(/is-fresh/);
  await expect(row).toContainText("ready");
  await row.getByRole("button", { name: "Details of greeter" }).click();
  await expect(row.locator("[data-plugin-details]")).toContainText(`Copied from ${source}`);

  // the copy is what is served: a record with a hash, not a link
  const plugins = await (await page.request.get(`${core}/api/v1/plugins`)).json();
  const greeter = plugins.plugins.find((p: { name: string }) => p.name === "greeter");
  expect(greeter.install.source_kind).toBe("folder");
  expect(greeter.install.link).toBe(false);
  expect(greeter.install.bundle).toBeTruthy();

  // the same version again says what it replaces
  const again = await openInstall(page);
  await again.getByLabel("Source").fill(source);
  await expect(again.locator('[data-replaces="unchanged"]')).toContainText(
    "greeter 1.2.0 is already installed, from this source, and the source has not changed",
  );
  await expect(again.locator("[data-install-confirm]")).toHaveText("Install again");
  // once the folder changes, installing the same version again copies it again
  fs.writeFileSync(path.join(source, "view/index.html"), "<html>second</html>");
  await again.getByLabel("Source").press("Enter");
  await expect(again.locator('[data-replaces="same"]')).toContainText(
    "greeter 1.2.0 is already installed. Installing replaces it for new reviews",
  );
  await expect(again.locator("[data-install-confirm]")).toHaveText("Replace");
  await again.locator("[data-install-confirm]").click();
  await expect(again.locator("[data-install-seen]")).toHaveCount(0);
  expect(await servedView(page, "greeter")).toBe("<html>second</html>");

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

test("a plugin whose view is not built is refused with what to do", async ({ page }) => {
  const source = pluginCopy("hello", "unbuilt", "1.0.0");
  fs.rmSync(path.join(source, "view/index.html"));
  const dialog = await openInstall(page);
  await dialog.getByLabel("Source").fill(source);
  await expect(dialog.locator(".install-error")).toContainText(
    "view/index.html not found; build the plugin first, so that its view is in the folder",
  );
  await expect(dialog.locator("[data-install-confirm]")).toHaveCount(0);
});

test("a link serves the folder live and offers to install a copy", async ({ page }) => {
  const source = pluginCopy("hello", "wip", "1.0.0");
  const dialog = await openInstall(page);
  await dialog.getByLabel("Source").fill(source);
  await expect(dialog.locator("[data-install-seen]")).toContainText("wip · 1.0.0");
  // linking is the other way to install, in the button's menu
  await dialog.getByRole("button", { name: "More ways to install" }).click();
  await expect(dialog.getByRole("menu")).toContainText("Changes to the folder appear the next time the view opens");
  await dialog.locator("[data-install-link]").click();
  await expect(dialog.locator("[data-install-seen]")).toHaveCount(0);

  const row = page.locator('[data-plugin-row="wip"]');
  await expect(row).toContainText("linked");
  await row.getByRole("button", { name: "Details of wip" }).click();
  await expect(row.locator("[data-plugin-details]")).toContainText(`Linked to ${source}`);

  // a copy from the row: the dialog opens looked at already, and says it replaces the link
  await row.getByRole("button", { name: "Install a copy of wip" }).click();
  const copy = page.locator("[data-install-panel]");
  await expect(copy.locator('[data-replaces="link"]')).toContainText(
    "wip 1.0.0 is already installed, as a link to this folder",
  );
  await copy.locator("[data-install-confirm]").click();
  await expect(copy.locator("[data-install-seen]")).toHaveCount(0);
  await expect(row).toContainText(`Copied from ${source}`);
});

test("a broken plugin can still be removed from its row", async ({ page }) => {
  const source = pluginCopy("hello", "broken", "1.0.0");
  await linkPlugin(page.request, source, "broken");
  // the folder changes under it into a manifest Pinrail no longer takes
  const manifest = JSON.parse(fs.readFileSync(path.join(source, "manifest.json"), "utf8"));
  fs.writeFileSync(path.join(source, "manifest.json"), JSON.stringify({ ...manifest, version: 1 }));
  // describing it reads the folder again, and refuses it
  const described = await page.request.get(`${core}/api/v1/plugins/broken/describe`);
  expect(described.status(), await described.text()).toBe(422);

  await page.goto("/#/plugins");
  const row = page.locator('[data-plugin-row="broken"]');
  // the badge says why, on hover, and nothing else on the row repeats it
  await expect(row).not.toContainText("version: value is not of type string");
  await row.locator("[data-plugin-broken]").hover();
  await expect(page.getByRole("tooltip")).toHaveText("version: value is not of type string");
  await expect(page.getByRole("tooltip")).toHaveClass(/is-danger/);
  // and its details open with it
  await row.getByRole("button", { name: "Details of broken" }).click();
  await expect(row.locator("[data-plugin-error]")).toHaveText(
    "This plugin is broken: version: value is not of type string",
  );
  await row.getByRole("button", { name: "Remove broken" }).click();
  await row.locator("[data-plugin-remove-confirm]").click();
  await expect(row).toHaveCount(0);
});

test("what is not a plugin is refused before anything runs", async ({ page }) => {
  const empty = scratch("pinrail-empty-");
  const dialog = await openInstall(page);
  await dialog.getByLabel("Source").fill(empty);
  await expect(dialog.locator(".install-error")).toContainText("manifest.json");
  await expect(dialog.locator("[data-install-confirm]")).toHaveCount(0);

  // Esc in the field clears it first, and leaves the settings open
  await dialog.getByLabel("Source").press("Escape");
  await expect(dialog.getByLabel("Source")).toHaveValue("");
  await expect(dialog.locator(".install-error")).toHaveCount(0);
  await expect(page.locator("[data-settings]")).toBeVisible();
  // once it is empty, Esc closes the settings
  await dialog.getByLabel("Source").press("Escape");
  await expect(page.locator("[data-settings]")).toHaveCount(0);
});

test("the buttons in a plugin's note do not fold its settings", async ({ page }) => {
  // Remove and Keep sit inside the row's note, which opens the settings
  // when clicked: pressing them must do only what they say
  const source = pluginCopy("hello", "planner", "1.0.0");
  const installed = await page.request.post(`${core}/api/v1/plugins/install`, { data: { source, link: true } });
  expect(installed.status(), await installed.text()).toBe(200);
  await page.goto("/#/plugins");
  const row = page.locator('[data-plugin-row="planner"]');
  await expect(row).toBeVisible();
  await expect(row).not.toHaveClass(/is-open/);

  await row.getByRole("button", { name: "Remove planner" }).click();
  await row.getByRole("button", { name: "Keep" }).click();
  await expect(row.locator("[data-plugin-remove-ask]")).toHaveCount(0);
  await expect(row, "Keep folded the settings open").not.toHaveClass(/is-open/);
});

test("an install the app could not do says why, and can be tried again", async ({ page }) => {
  const source = pluginCopy("hello", "wobbly", "1.0.0");
  await page.route(`${core}/api/v1/plugins/install`, (route) =>
    route.fulfill({ status: 500, contentType: "text/plain", body: "the server is restarting" }),
  );
  const dialog = await openInstall(page);
  await dialog.getByLabel("Source").fill(source);
  await dialog.locator("[data-install-confirm]").click();

  await expect(dialog.locator(".install-error")).toContainText("request failed (500)");
  await expect(dialog.getByLabel("Source")).toHaveValue(source);
  await expect(dialog.locator("[data-install-confirm]")).toBeEnabled();
});
