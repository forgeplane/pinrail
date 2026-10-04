import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

// The view alone, under the harness: no app, no CLI. `mountPlugin` serves
// this folder as the app would and plays the shell's side of the protocol.
const dir = path.resolve(__dirname, "..");
const basic = () => fixture(path.join(dir, "fixtures", "basic.json"));

test("renders the payload, and hands over the answer as the app does", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: basic() });
  await expect(plugin.frame.locator("p").first()).toContainText("3 commits");
  // the view connects once, however often its components mount
  expect((await plugin.messages()).filter((m) => m.type === "ready")).toHaveLength(1);

  await plugin.frame.getByRole("button", { name: "Yes" }).click();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over: yes");

  // asked for its decision, checked against the decision schema, accepted
  const handed = await plugin.handOver();
  expect(handed).toMatchObject({ decision: { data: { ok: true } } });
});

test("asks for an answer before handing over, and shows what the app refuses", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: basic() });
  // no answer yet: the view hands nothing over, and says why
  expect(await plugin.handOver()).toEqual({ deferred: true });
  await expect(plugin.frame.locator("#errors")).toHaveText("Choose yes or no first.");

  await plugin.frame.getByRole("button", { name: "No" }).click();
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ ok: false });

  await plugin.sendViolations([{ path: "/ok", message: "value is not of type boolean" }]);
  await expect(plugin.frame.locator("#errors")).toContainText("/ok: value is not of type boolean");
});

test("a decided review renders read-only", async ({ page }) => {
  const decided = {
    ...basic(),
    decision: { decided_by: "you", decided_at: "2026-09-16T09:00:00Z", data: { ok: true } },
  };
  const plugin = await mountPlugin(page, dir, { review: decided, readonly: true });
  await expect(plugin.frame.locator("body")).toContainText("Decided: yes");
  await expect(plugin.frame.getByRole("button", { name: "Yes" })).toHaveCount(0);
});

test("a review that ended without a decision does not read as a no", async ({ page }) => {
  // withdrawn or expired: read-only, and nobody answered
  const withdrawn = { ...basic(), status: "withdrawn", decision: null };
  const plugin = await mountPlugin(page, dir, { review: withdrawn, readonly: true });
  await expect(plugin.frame.locator("body")).toContainText("Closed without a decision (withdrawn)");
  await expect(plugin.frame.locator("body")).not.toContainText("Decided");
});

test("an answer pressed from the keyboard keeps the focus on its button", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: basic() });
  const yes = plugin.frame.getByRole("button", { name: "Yes" });
  await yes.focus();
  await plugin.frame.locator("body").press("Enter");
  await expect(yes).toHaveAttribute("aria-pressed", "true");
  await expect(yes, "focus fell off the button").toBeFocused();
});

test("a draft of another shape, as an earlier release could have kept, leaves the view empty", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: basic(), draft: { verdict: "ship" } });
  await expect(plugin.frame.getByRole("button", { name: "Yes" })).toHaveAttribute("aria-pressed", "false");
  await expect(plugin.frame.getByRole("button", { name: "No" })).toHaveAttribute("aria-pressed", "false");
  expect(await plugin.handOver()).toEqual({ deferred: true });
});
