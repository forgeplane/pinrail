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
  await expect(page.locator("[data-handover]")).toHaveText("Hand over: yes");
  await page.locator("[data-handover]").click();
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
  await page.locator("[data-handover]").click();
  const result = await waiter.done;
  expect(result.code).toBe(0);
  expect(JSON.parse(result.stdout).decision.data).toEqual({ ok: false });
});

test("an icon reaches a gate view across the sandbox and paints", async ({ page }) => {
  // A view runs with an opaque origin, so a CSS mask image is a cross-origin
  // fetch. Only a real server proves it gets through: the isolated harness
  // answers these requests itself and cannot show the policy that governs them.
  const served: Record<string, number> = {};
  const refused: string[] = [];
  page.on("response", (r) => {
    if (r.url().includes("/sdk/v1/icons/")) served[r.url().split("/").pop()!] = r.status();
  });
  page.on("console", (m) => {
    if (/CORS|Content Security Policy|blocked/i.test(m.text())) refused.push(m.text());
  });

  const payload = tmpFile("p.json", JSON.stringify({ message: "Push?" }));
  const waiter = startWaiter(["create", "hello", "--title", "Icons", "--data", payload, "--wait"]);
  const id = await waiter.gateId;

  await page.goto(gateUrl(id));
  const frame = page.frameLocator("#plugin-frame");
  const mark = frame.getByRole("button", { name: "Yes" }).locator(".wi");
  await expect(mark).toHaveAttribute("data-icon", "check");
  await expect.poll(() => served["check.svg"]).toBe(200);
  expect(refused, "the browser refused an icon").toEqual([]);

  const painted = await mark.evaluate((el) => {
    const box = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return {
      width: Math.round(box.width),
      colour: style.backgroundColor,
      textColour: getComputedStyle(el.closest("button")!).color,
    };
  });
  expect(painted.width).toBeGreaterThan(8);
  expect(painted.colour).toBe(painted.textColour);

  waiter.proc.kill();
});
