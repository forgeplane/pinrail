import { expect, test } from "@playwright/test";
import { execFileSync, spawn } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import { scratch } from "./scratch.cjs";

// `pinrail-plugin dev`, the shell in a browser: a plugin from create, with
// its sample and a decided fixture beside it.
const sdk = path.resolve(import.meta.dirname, "..");
const bin = path.join(sdk, "bin", "pinrail-plugin.mjs");

const freePort = () =>
  new Promise<number>((resolve) => {
    const server = net.createServer();
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address() as net.AddressInfo;
      server.close(() => resolve(port));
    });
  });

test("the menu offers the samples and the fixtures, marks a decided one, and loads the one chosen", async ({ page }) => {
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

  const port = await freePort();
  const shell = spawn(process.execPath, [bin, "dev", dir, "--port", String(port), "--no-open"], { stdio: "pipe" });
  try {
    await expect
      .poll(async () => (await fetch(`http://127.0.0.1:${port}/dev/manifest`).catch(() => null))?.status)
      .toBe(200);
    await page.goto(`http://127.0.0.1:${port}/`);
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
  } finally {
    shell.kill();
  }
});
