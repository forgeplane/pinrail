import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

// A scaffolded plugin, before a line of it is changed, under the harness:
// what `create` writes has to work, not only exist.
const sdk = path.resolve(import.meta.dirname, "..");
const bin = path.join(sdk, "bin", "pinrail-plugin.mjs");

function scaffold(name: string, template: "plain" | "vite" | "react" | "vue"): string {
  const dir = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "pinrail-scaffold-")), name);
  execFileSync(process.execPath, [bin, "create", name, "--template", template, "--dir", dir, "--sdk", `file:${sdk}`], { stdio: "pipe" });
  return dir;
}

async function decides(page: any, dir: string) {
  const plugin = await mountPlugin(page, dir, { gate: fixture(path.join(dir, "fixtures", "basic.json")) });
  await expect(plugin.frame.locator("p").first()).toContainText("3 commits");
  await plugin.frame.getByRole("button", { name: "Yes" }).click();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over: yes");
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ ok: true });
}

test("the plain scaffold renders its fixture and hands over a decision", async ({ page }) => {
  await decides(page, scaffold("triage", "plain"));
});

test("the vite scaffold type-checks against the package, builds, and does the same", async ({ page }) => {
  test.slow();
  const dir = scaffold("fancy", "vite");
  // vite and typescript come from the registry; the package from this checkout
  execFileSync("npm", ["install", "--no-audit", "--no-fund"], { cwd: dir, stdio: "pipe" });
  execFileSync("npm", ["run", "build"], { cwd: dir, stdio: "pipe" });
  expect(fs.existsSync(path.join(dir, "view", "index.html"))).toBe(true);
  await decides(page, dir);
});

/** A framework's scaffold: installed, type-checked and built, and passing
 *  the tests it comes with (read-only, violations, an answer missing),
 *  which are the same tests for every template. */
function passesItsOwnTests(dir: string) {
  execFileSync("npm", ["install", "--no-audit", "--no-fund"], { cwd: dir, stdio: "pipe" });
  execFileSync("npm", ["test"], { cwd: dir, stdio: "pipe" });
  expect(fs.existsSync(path.join(dir, "view", "index.html"))).toBe(true);
}

test("the react scaffold type-checks, builds and passes its own tests", async ({ page }) => {
  test.setTimeout(180_000);
  const dir = scaffold("fancy_react", "react");
  passesItsOwnTests(dir);
  await decides(page, dir);
});

test("the vue scaffold type-checks, builds and passes its own tests", async ({ page }) => {
  test.setTimeout(180_000);
  const dir = scaffold("fancy_vue", "vue");
  passesItsOwnTests(dir);
  await decides(page, dir);
});
