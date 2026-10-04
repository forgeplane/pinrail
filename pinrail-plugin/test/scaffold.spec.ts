import { expect, test } from "@playwright/test";
import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";
import { withDevShell } from "./dev-shell";
import { scratch } from "./scratch.cjs";

// A scaffolded plugin, before a line of it is changed, under the harness:
// what `create` writes has to work, not only exist.
const sdk = path.resolve(import.meta.dirname, "..");
const bin = path.join(sdk, "bin", "pinrail-plugin.mjs");

function scaffold(name: string, template: "plain" | "vite" | "react"): string {
  const dir = path.join(scratch("pinrail-scaffold-"), name);
  execFileSync(process.execPath, [bin, "create", name, "--template", template, "--dir", dir, "--sdk", `file:${sdk}`], {
    stdio: "pipe",
  });
  return dir;
}

// `check` runs the pinrail command, which the SDK's own CI does not install
const pinrail = spawnSync("pinrail", ["--version"]).status === 0;

/** The folder passes `pinrail-plugin check`, and runs under `dev`. */
async function checksAndRuns(page: any, dir: string) {
  if (pinrail) {
    const checked = spawnSync(process.execPath, [bin, "check", dir], { encoding: "utf8" });
    expect(checked.status, checked.stdout + checked.stderr).toBe(0);
  } else {
    test.info().annotations.push({ type: "skipped", description: "pinrail-plugin check: no pinrail command" });
  }
  await withDevShell(dir, async (url) => {
    await page.goto(url);
    await expect(page.frameLocator("#frame").getByRole("button", { name: "Yes" })).toBeVisible();
  });
}

async function decides(page: any, dir: string) {
  const plugin = await mountPlugin(page, dir, {
    review: fixture(path.join(dir, "samples", `${path.basename(dir)}.json`)),
  });
  await expect(plugin.frame.locator("p").first()).toContainText("3 commits");
  await plugin.frame.getByRole("button", { name: "Yes" }).click();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over: yes");
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ ok: true });
}

test("the plain scaffold renders its sample, hands over a decision, passes check and runs under dev", async ({
  page,
}) => {
  const dir = scaffold("triage", "plain");
  await decides(page, dir);
  await checksAndRuns(page, dir);
});

test("the vite scaffold type-checks against the package, builds, and does the same", async ({ page }) => {
  test.slow();
  const dir = scaffold("fancy", "vite");
  // vite and typescript come from the registry; the package from this checkout
  execFileSync("npm", ["install", "--no-audit", "--no-fund"], { cwd: dir, stdio: "pipe" });
  execFileSync("npm", ["run", "build"], { cwd: dir, stdio: "pipe" });
  expect(fs.existsSync(path.join(dir, "view", "index.html"))).toBe(true);
  await decides(page, dir);
  await checksAndRuns(page, dir);
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
  await checksAndRuns(page, dir);
});
