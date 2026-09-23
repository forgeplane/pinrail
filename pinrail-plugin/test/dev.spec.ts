import { expect, test } from "@playwright/test";
import { execFileSync, spawn } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";

// `pinrail-plugin dev`, the shell in a browser: a plugin from create, with
// a pending fixture and a decided one beside it.
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

test("the fixture menu marks a decided fixture, and choosing one loads it", async ({ page }) => {
  const dir = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "pinrail-dev-")), "triage");
  execFileSync(process.execPath, [bin, "create", "triage", "--dir", dir, "--sdk", `file:${sdk}`], { stdio: "pipe" });
  const basic = JSON.parse(fs.readFileSync(path.join(dir, "fixtures", "basic.json"), "utf8"));
  // the decided round sorts first, and has the pending one's title
  fs.writeFileSync(
    path.join(dir, "fixtures", "a-decided.json"),
    JSON.stringify({ ...basic, decision: { decided_by: "you", decided_at: "2026-09-16T09:00:00Z", data: { ok: false } } }),
  );

  const port = await freePort();
  const shell = spawn(process.execPath, [bin, "dev", dir, "--port", String(port), "--no-open"], { stdio: "pipe" });
  try {
    await expect.poll(async () => (await fetch(`http://127.0.0.1:${port}/dev/manifest`).catch(() => null))?.status).toBe(200);
    await page.goto(`http://127.0.0.1:${port}/`);
    const menu = page.locator("#fixture");
    await expect(menu.locator("option")).toHaveText([`${basic.title} (decided)`, basic.title]);

    const view = page.frameLocator("#frame");
    await menu.selectOption("a-decided.json");
    await expect(view.locator("body")).toContainText("Decided: no");
    // the frame shows the same page for every fixture; choosing another still loads it again
    await menu.selectOption("basic.json");
    await expect(view.getByRole("button", { name: "Yes" })).toBeVisible();
  } finally {
    shell.kill();
  }
});
