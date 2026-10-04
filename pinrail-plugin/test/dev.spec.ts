import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { withDevShell } from "./dev-shell";
import { scratch } from "./scratch.cjs";

// `pinrail-plugin dev`, the shell in a browser: a plugin from create, with
// its sample and a decided fixture beside it.
const sdk = path.resolve(import.meta.dirname, "..");
const bin = path.join(sdk, "bin", "pinrail-plugin.mjs");

test("the menu offers the samples and the fixtures, marks a decided one, and loads the one chosen", async ({
  page,
}) => {
  const dir = path.join(scratch("pinrail-dev-"), "triage");
  execFileSync(process.execPath, [bin, "create", "triage", "--dir", dir, "--sdk", `file:${sdk}`], { stdio: "pipe" });
  const sample = JSON.parse(fs.readFileSync(path.join(dir, "samples", "triage.json"), "utf8"));
  // a decided round, with the pending one's title
  fs.mkdirSync(path.join(dir, "fixtures"));
  fs.writeFileSync(
    path.join(dir, "fixtures", "decided.json"),
    JSON.stringify({
      ...sample,
      decision: { decided_by: "you", decided_at: "2026-09-16T09:00:00Z", data: { ok: false } },
    }),
  );

  await withDevShell(dir, async (url) => {
    await page.goto(url);
    const menu = page.locator("#fixture");
    // the sample first, as the one the app would show
    await expect(menu.locator("option")).toHaveText([sample.title, `${sample.title} (decided)`]);
    const view = page.frameLocator("#frame");
    await expect(view.getByRole("button", { name: "Yes" })).toBeVisible();

    await menu.selectOption("fixtures/decided.json");
    await expect(view.locator("body")).toContainText("Decided: no");
    // the frame shows the same page for every fixture; choosing another still loads it again
    await menu.selectOption("samples/triage.json");
    await expect(view.getByRole("button", { name: "Yes" })).toBeVisible();
  });
});

test("the view is set in the app's typeface, and nothing is fetched from off the machine", async ({ page }) => {
  const dir = path.join(scratch("pinrail-dev-"), "typeface");
  execFileSync(process.execPath, [bin, "create", "typeface", "--dir", dir, "--sdk", `file:${sdk}`], { stdio: "pipe" });
  const away: string[] = [];
  page.on("request", (r) => new URL(r.url()).hostname !== "127.0.0.1" && away.push(r.url()));

  await withDevShell(dir, async (url) => {
    await page.goto(url);
    await expect(page.frameLocator("#frame").getByRole("button", { name: "Yes" })).toBeVisible();
    const view = page.frames().find((f) => f.url().includes("/plugin/"))!;

    const loaded = await view.evaluate(async () => {
      await document.fonts.ready;
      return [...document.fonts].some((f) => f.family.replace(/"/g, "") === "Inter Variable" && f.status === "loaded");
    });
    expect(loaded).toBe(true);
    expect(away).toEqual([]);
  });
});
