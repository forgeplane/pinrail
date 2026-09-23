import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

// The view alone, under the harness: no app, no CLI. The same test runs
// against every framework's build of this plugin, so it looks only at what
// the person sees: text, roles and labels.
const dir = path.resolve(__dirname, "..");
const deploy = () => fixture(path.join(dir, "fixtures", "deploy.json"));

test("shows what goes out and how the checks went", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: deploy() });
  const f = plugin.frame;
  await expect(f.getByRole("heading", { level: 1 })).toHaveText("payments-api v2.4.1");
  await expect(f.getByText("Deploy to production")).toBeVisible();
  await expect(f.getByRole("list", { name: "Changes" }).getByRole("listitem")).toHaveCount(3);
  await expect(f.getByRole("list", { name: "Changes" }).getByText("risky")).toHaveCount(1);
  const checks = f.getByRole("list", { name: "Checks" }).getByRole("listitem");
  await expect(checks).toHaveCount(4);
  await expect(checks.filter({ hasText: "Canary" })).toHaveAttribute("data-passed", "false");
  await expect(checks.filter({ hasText: "Canary" })).toContainText("p99 latency 180 ms over the baseline");
});

test("ship with a key, and a note, handed over when the app collects", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: deploy() });
  const f = plugin.frame;
  await expect(f.getByRole("button", { name: /^Ship/ })).toBeVisible();
  // a key pressed while the app has the focus, forwarded to the view as the app does
  await plugin.sendKey("s");
  await expect(f.getByRole("button", { name: /^Ship/ })).toHaveAttribute("aria-pressed", "true");
  await expect.poll(() => plugin.lastStatus()).toBe("Ship v2.4.1");
  await f.getByLabel("Note to the agent").fill("Watch the canary for an hour");
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ verdict: "ship", note: "Watch the canary for an hour" });
});

test("asks for a verdict first, and shows what the app refuses", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: deploy() });
  const f = plugin.frame;
  await expect(f.getByRole("button", { name: /^Hold/ })).toBeVisible();
  await plugin.collect();
  await expect(f.getByRole("alert")).toHaveText("Choose ship or hold first.");

  await f.getByRole("button", { name: /^Hold/ }).click();
  await expect.poll(() => plugin.lastStatus()).toBe("Hold the deploy");
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ verdict: "hold" });

  await plugin.sendViolations([{ path: "/verdict", message: "\"later\" is not one of [\"ship\",\"hold\"]" }]);
  await expect(f.getByRole("alert")).toContainText("/verdict:");
});

test("a draft comes back as it was left", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: deploy(), draft: { verdict: "hold", note: "Wait for the canary" } });
  const f = plugin.frame;
  await expect(f.getByRole("button", { name: /^Hold/ })).toHaveAttribute("aria-pressed", "true");
  await expect(f.getByLabel("Note to the agent")).toHaveValue("Wait for the canary");
  await f.getByRole("button", { name: /^Ship/ }).click();
  await expect.poll(() => plugin.lastDraft()).toEqual({ verdict: "ship", note: "Wait for the canary" });
});

test("a decided deploy renders read-only", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: fixture(path.join(dir, "fixtures", "deploy.decided.json")), readonly: true });
  const f = plugin.frame;
  await expect(f.getByText("Held: Wait for the canary to settle")).toBeVisible();
  await expect(f.getByRole("button", { name: /^Ship/ })).toHaveCount(0);
});
