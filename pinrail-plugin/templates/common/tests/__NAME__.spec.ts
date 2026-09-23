import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

// The view alone, under the harness: no app, no CLI. `mountPlugin` serves
// this folder as the app would and plays the shell's side of the protocol.
const dir = path.resolve(__dirname, "..");
const basic = () => fixture(path.join(dir, "fixtures", "basic.json"));

test("renders the payload, and hands over the answer when the shell collects", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: basic() });
  await expect(plugin.frame.locator("p").first()).toContainText("3 commits");

  await plugin.frame.getByRole("button", { name: "Yes" }).click();
  await plugin.frame.getByPlaceholder("comment (optional)").fill("after the rebase");
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over: yes");

  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ ok: true, comment: "after the rebase" });
});

test("asks for an answer before handing over, and shows what the app refuses", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: basic() });
  await plugin.collect();
  await expect(plugin.frame.locator("#errors")).toHaveText("Choose yes or no first.");

  await plugin.frame.getByRole("button", { name: "No" }).click();
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ ok: false });

  await plugin.sendViolations([{ path: "/ok", message: "value is not of type boolean" }]);
  await expect(plugin.frame.locator("#errors")).toContainText("/ok: value is not of type boolean");
});

test("a decided review renders read-only", async ({ page }) => {
  const decided = { ...basic(), decision: { decided_by: "you", decided_at: "2026-09-16T09:00:00Z", data: { ok: true, comment: "go" } } };
  const plugin = await mountPlugin(page, dir, { gate: decided, readonly: true });
  await expect(plugin.frame.locator("body")).toContainText("Decided: yes");
  await expect(plugin.frame.getByRole("button", { name: "Yes" })).toHaveCount(0);
});
