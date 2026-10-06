import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "pinrail-sdk/testing";

// The view alone, under the harness: no app, no CLI. `mountPlugin` serves
// this folder as the app would and plays the shell's side of the protocol.
const dir = path.resolve(__dirname, "..");
const sample = () => fixture(path.join(dir, "samples", "__NAME__.json"));

test("renders the payload, and hands over the answer as the app does", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: sample() });
  await expect(plugin.frame.locator("p").first()).toContainText("3 commits");
  // the view connects once, however often its components mount
  expect((await plugin.messages()).filter((m) => m.type === "ready")).toHaveLength(1);

  await plugin.frame.getByRole("button", { name: "Yes" }).click();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over: yes");

  // asked for its decision, checked against the decision schema, accepted
  const handed = await plugin.handOver();
  expect(handed).toMatchObject({ decision: { data: { ok: true } } });
});

test("asks for an answer before handing over, and leaves a refused decision to the app", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: sample() });
  // no answer yet: the view hands nothing over, and says why
  expect(await plugin.handOver()).toEqual({ deferred: true });
  await expect(plugin.frame.locator("#errors")).toHaveText("Choose yes or no first.");

  await plugin.frame.getByRole("button", { name: "No" }).click();
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ ok: false });

  // the app lists a refused decision's violations under the view, so the
  // view does not repeat them
  await plugin.sendViolations([{ path: "/ok", message: "value is not of type boolean" }]);
  await page.waitForTimeout(200);
  await expect(plugin.frame.locator("#errors")).toHaveText("");
});

test("a decided review renders read-only", async ({ page }) => {
  const decided = {
    ...sample(),
    decision: { decided_by: "you", decided_at: "2026-09-16T09:00:00Z", data: { ok: true } },
  };
  const plugin = await mountPlugin(page, dir, { review: decided, readonly: true });
  await expect(plugin.frame.locator("body")).toContainText("Decided: yes");
  await expect(plugin.frame.getByRole("button", { name: "Yes" })).toHaveCount(0);
});

test("a review that ended without a decision does not read as a no", async ({ page }) => {
  // withdrawn or expired: read-only, and nobody answered
  const withdrawn = { ...sample(), status: "withdrawn", decision: null };
  const plugin = await mountPlugin(page, dir, { review: withdrawn, readonly: true });
  await expect(plugin.frame.locator("body")).toContainText("Closed without a decision (withdrawn)");
  await expect(plugin.frame.locator("body")).not.toContainText("Decided");
});

test("an answer pressed from the keyboard keeps the focus on its button", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: sample() });
  const yes = plugin.frame.getByRole("button", { name: "Yes" });
  await yes.focus();
  await plugin.frame.locator("body").press("Enter");
  await expect(yes).toHaveAttribute("aria-pressed", "true");
  await expect(yes, "focus fell off the button").toBeFocused();
});

test("a draft of another shape, as an earlier release could have kept, leaves the view empty", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: sample(), draft: { verdict: "ship" } });
  await expect(plugin.frame.getByRole("button", { name: "Yes" })).toHaveAttribute("aria-pressed", "false");
  await expect(plugin.frame.getByRole("button", { name: "No" })).toHaveAttribute("aria-pressed", "false");
  expect(await plugin.handOver()).toEqual({ deferred: true });
});
