import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "../../../wicket_sdk/testing/playwright";

const dir = path.resolve(__dirname, "..");
const push = () => fixture(path.join(dir, "fixtures", "push.json"));

test("asks the question and submits yes with the comment", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: push() });
  await expect(plugin.frame.locator("p").first()).toHaveText(push().payload.message);
  await plugin.frame.getByPlaceholder("comment (optional)").fill("after the rebase");
  await plugin.frame.getByRole("button", { name: "Yes" }).click();
  expect(await plugin.nextSubmit()).toEqual({ ok: true, comment: "after the rebase" });
});

test("no without a comment submits only ok", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: push() });
  await plugin.frame.getByRole("button", { name: "No" }).click();
  expect(await plugin.nextSubmit()).toEqual({ ok: false });
});

test("collect presses yes; violations are shown; submitted renders read-only", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: push() });
  await expect(plugin.frame.getByRole("button", { name: "Yes" })).toBeVisible();
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ ok: true });

  await plugin.sendViolations([{ path: "/ok", message: "value is not of type boolean" }]);
  await expect(plugin.frame.locator("#errors")).toHaveText("/ok: value is not of type boolean");

  await plugin.sendSubmitted({ decided_by: "alice", decided_at: "2026-09-11T10:00:00Z", data: { ok: true, comment: "go" } });
  await expect(plugin.frame.locator("p").last()).toContainText("Decided: yes — go");
  await expect(plugin.frame.getByRole("button", { name: "Yes" })).toHaveCount(0);
});

test("a typed comment is drafted and restored after a reload", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: push() });
  await plugin.frame.getByPlaceholder("comment (optional)").fill("keep this");
  await expect.poll(() => plugin.lastDraft()).toEqual({ comment: "keep this" });
  await plugin.reload();
  await plugin.reinit();
  await expect(plugin.frame.getByPlaceholder("comment (optional)")).toHaveValue("keep this");
});

test("follows the shell's theme without losing what was typed", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: push() });
  const comment = plugin.frame.getByPlaceholder("comment (optional)");
  await comment.fill("keep this");

  await plugin.send({ type: "appearance", theme: "light" });
  await expect(plugin.frame.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(comment).toHaveValue("keep this");

  await plugin.send({ type: "appearance", theme: "dark" });
  await expect(plugin.frame.locator("html")).toHaveAttribute("data-theme", "dark");
});

test("renders a decided gate read-only", async ({ page }) => {
  const gate = { ...push(), decision: { decided_by: "alice", decided_at: "2026-09-11T10:00:00Z", data: { ok: false } } };
  const plugin = await mountPlugin(page, dir, { gate, readonly: true });
  await expect(plugin.frame.locator("p").last()).toContainText("Decided: no");
  await expect(plugin.frame.locator("button")).toHaveCount(0);
});
