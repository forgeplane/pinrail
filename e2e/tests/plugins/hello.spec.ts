import { expect, test } from "@playwright/test";
import { gateUrl, startWaiter, tmpFile } from "../../helpers/wicket";

test("the hello sample asks a question and returns yes with a comment", async ({ page }) => {
  const payload = tmpFile("p.json", JSON.stringify({ message: "3 commits on top of main. Push?" }));
  const waiter = startWaiter(["create", "hello", "--title", "Push the branch?", "--data", payload, "--wait"]);
  const id = await waiter.gateId;

  await page.goto(gateUrl(id));
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator("p").first()).toHaveText("3 commits on top of main. Push?");
  await frame.getByPlaceholder("comment (optional)").fill("after the rebase");
  await frame.getByRole("button", { name: "Yes" }).click();
  await expect(frame.locator("p").last()).toContainText("Decided: yes");

  const result = await waiter.done;
  expect(result.code).toBe(0);
  expect(JSON.parse(result.stdout).decision.data).toEqual({ ok: true, comment: "after the rebase" });
});

test("a plugin that violates its schema is refused and told where", async ({ page }) => {
  // hello's schema forbids extra keys; drive a bad submit through the bridge by hand
  const payload = tmpFile("p.json", JSON.stringify({ message: "q" }));
  const waiter = startWaiter(["create", "hello", "--title", "bad submit", "--data", payload, "--wait", "--timeout", "30"]);
  const id = await waiter.gateId;

  await page.goto(gateUrl(id));
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.getByRole("button", { name: "Yes" })).toBeVisible();
  await frame.locator("body").evaluate(() => {
    parent.postMessage({ wicket: 1, type: "submit", data: { ok: "maybe", extra: 1 } }, "*");
  });
  await expect(page.locator("#gate-violations")).toContainText("/ok");
  await expect(frame.locator("#errors")).toContainText("/ok: value is not of type boolean");

  await frame.getByRole("button", { name: "No" }).click();
  const result = await waiter.done;
  expect(result.code).toBe(0);
  expect(JSON.parse(result.stdout).decision.data).toEqual({ ok: false });
});
