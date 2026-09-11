import { expect, test, type Page } from "@playwright/test";
import { createListGate, gateUrl, wicketJson } from "../../helpers/wicket";

const plugin = (page: Page) => page.frameLocator("#plugin-frame");

test("accept, reject with a reason, add a note, submit; the waiter gets exactly that", async ({ page }) => {
  const waiter = createListGate("list: full round");
  const id = await waiter.gateId;

  await page.goto(gateUrl(id));
  await expect(page.locator("h1")).toHaveText("list: full round");
  const frame = plugin(page);
  await expect(frame.locator(".item", { hasText: "#1" })).toContainText("do_save dedups");
  await frame.locator('[data-id="1"] button', { hasText: "Accept" }).click();
  await frame.locator('[data-id="2"] button', { hasText: "Reject" }).click();
  await frame.getByLabel("note for item 2").fill("not worth a comment");
  await frame.getByRole("button", { name: "+ add a note of your own" }).click();
  await frame.getByLabel("addition 1", { exact: true }).fill("please also check the migration");
  await page.getByLabel("note to the agent").fill("next round: tickets only");
  await expect(frame.locator(".footer")).toContainText("1 accepted · 1 rejected · 0 undecided");
  await frame.getByRole("button", { name: "Submit decisions" }).click();

  await expect(page.locator("#gate-decision")).toContainText("next round: tickets only");
  await expect(frame.locator(".done")).toContainText("1 accepted");

  const result = await waiter.done;
  expect(result.code, result.stderr).toBe(0);
  const envelope = JSON.parse(result.stdout);
  expect(envelope.status).toBe("decided");
  expect(envelope.agent_note).toBe("next round: tickets only");
  expect(envelope.decision.data).toEqual({
    decisions: [
      { id: 1, action: "accept" },
      { id: 2, action: "reject", note: "not worth a comment" },
    ],
    undecided: [],
    additions: [{ body: "please also check the migration" }],
  });
});

test("items left undecided are confirmed and reported as undecided", async ({ page }) => {
  const waiter = createListGate("list: partial");
  const id = await waiter.gateId;

  await page.goto(gateUrl(id));
  const frame = plugin(page);
  await frame.locator('[data-id="1"] button', { hasText: "Accept" }).click();
  await frame.getByRole("button", { name: "Submit decisions" }).click();
  await expect(frame.locator(".footer")).toContainText("1 left undecided");
  await frame.getByRole("button", { name: "Submit anyway" }).click();

  const result = await waiter.done;
  expect(result.code).toBe(0);
  expect(JSON.parse(result.stdout).decision.data).toEqual({ decisions: [{ id: 1, action: "accept" }], undecided: [2] });
});

test("a draft survives a reload and the page is read-only after the decision", async ({ page }) => {
  const waiter = createListGate("list: draft");
  const id = await waiter.gateId;

  await page.goto(gateUrl(id));
  const frame = plugin(page);
  await frame.locator('[data-id="1"] button', { hasText: "Accept" }).click();
  await frame.getByLabel("note for item 1").fill("mention the COALESCE");
  await expect(frame.locator('[data-id="1"].accepted')).toBeVisible();
  await page.waitForFunction((key) => (sessionStorage.getItem(key) || "").includes("COALESCE"), `wicket:draft:${id}`);

  await page.reload();
  await expect(frame.locator('[data-id="1"].accepted')).toBeVisible();
  await expect(frame.getByLabel("note for item 1")).toHaveValue("mention the COALESCE");

  await frame.getByRole("button", { name: "accept all undecided" }).click();
  await page.keyboard.press("Control+Enter");
  await expect(page.locator("#gate-decision")).toBeVisible();
  expect((await waiter.done).code).toBe(0);

  await page.reload();
  await expect(frame.locator(".done")).toContainText("2 accepted");
  await expect(frame.getByRole("button", { name: "Submit decisions" })).toHaveCount(0);
  await expect(page.locator("#agent-note-form")).toHaveCount(0);
});

test("the inbox shows the gate, the title carries the count, history lists the decision", async ({ page }) => {
  const waiter = createListGate("list: inbox");
  const id = await waiter.gateId;

  await page.goto(`${gateUrl(id).replace(/\/gates\/.*$/, "/")}`);
  await expect(page.locator(`#gate-${id}`)).toContainText("list: inbox");
  await expect(page).toHaveTitle(/^\(\d+\) Inbox · wicket$/);

  await page.locator(`#gate-${id}`).click();
  await expect(page).toHaveURL(gateUrl(id));
  wicketJson(["withdraw", id]);
  await expect(page.locator("#gate-withdrawn")).toBeVisible();
  await expect(plugin(page).locator(".done")).toContainText("withdrawn");
  expect((await waiter.done).code).toBe(3);

  await page.goto(`${gateUrl(id).replace(/\/gates\/.*$/, "/history?status=withdrawn")}`);
  await expect(page.locator(`#row-${id}`)).toContainText("withdrawn");
});
