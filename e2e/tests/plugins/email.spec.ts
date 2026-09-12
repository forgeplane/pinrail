import { expect, test } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { gateUrl, startWaiter, tmpFile } from "../../helpers/wicket";

const renewals = JSON.parse(
  fs.readFileSync(path.resolve(__dirname, "../../../plugins/email/fixtures/renewals.json"), "utf8"),
).payload;

test("an edited draft and its instructions come back through the schema", async ({ page }) => {
  const payload = tmpFile("email.json", JSON.stringify(renewals));
  const waiter = startWaiter(["create", "email", "--title", "renewals", "--data", payload, "--wait"]);
  const id = await waiter.gateId;

  await page.goto(gateUrl(id));
  const frame = page.frameLocator("#plugin-frame");
  const first = frame.locator('[data-draft="northwind"]');
  await expect(first.locator("[data-body]")).toContainText("Your team's usage is up 40%");

  // Edit the body, so the change has to survive the round trip and the schema.
  await first.locator('[data-act="edit"]').click();
  const area = first.locator("textarea");
  await area.fill((await area.inputValue()).replace("at your earliest convenience", "this week"));
  await first.locator('[data-act="edit"]').click();
  await expect(first.locator("del")).toHaveText("at your earliest convenience,");

  // Hang an instruction on a passage.
  await first.locator("[data-body]").evaluate((body) => {
    const node = [...body.childNodes].find((n) => n.textContent!.includes("I wanted to reach out"))!;
    const at = node.textContent!.indexOf("I wanted to reach out");
    const range = document.createRange();
    range.setStart(node, at);
    range.setEnd(node, at + "I wanted to reach out".length);
    const selection = document.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    document.dispatchEvent(new Event("selectionchange"));
  });
  await frame.locator("#pick button").click();
  await first.locator('input[data-act="mark-note"]').fill("we never say reach out");

  await first.getByRole("button", { name: "Send" }).click();
  await frame.locator('[data-draft="brightside"]').getByRole("button", { name: "Revise" }).click();
  await frame.locator('[data-draft="kestrel"]').getByRole("button", { name: "Discard" }).click();
  await expect(page.locator("[data-handover]")).toHaveText("Hand over: send 1, revise 1, discard 1");

  await page.locator("#agent_note").fill("hold everything until finance confirms Kestrel");
  await page.locator("[data-handover]").click();
  await expect(page.locator("#gate-decision")).toBeVisible();

  const result = await waiter.done;
  expect(result.code).toBe(0);
  const data = JSON.parse(result.stdout).decision.data;

  expect(data.undecided).toEqual([]);
  const sent = data.drafts.find((d: any) => d.id === "northwind");
  expect(sent.action).toBe("send");
  expect(sent.subject).toBe("Your Acme renewal on 12 October");
  expect(sent.body).toContain("this week");
  expect(sent.body).not.toContain("at your earliest convenience");
  expect(sent.edits).toEqual([{ from: "at your earliest convenience,", to: "this week," }]);
  expect(sent.comments).toEqual([{ quote: "I wanted to reach out", note: "we never say reach out" }]);
  expect(data.drafts.map((d: any) => d.action)).toEqual(["send", "revise", "discard"]);

  // The note to the agent rides on the gate, not on any one draft.
  expect(JSON.parse(result.stdout).agent_note).toBe("hold everything until finance confirms Kestrel");
});

test("a decision the schema refuses is refused, and the gate stays pending", async ({ page }) => {
  const payload = tmpFile("email.json", JSON.stringify(renewals));
  const waiter = startWaiter(["create", "email", "--title", "renewals", "--data", payload, "--wait"]);
  const id = await waiter.gateId;

  await page.goto(gateUrl(id));
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator('[data-draft="northwind"]')).toBeVisible();

  // Submit an action the schema does not allow, past the view, through the bridge.
  await frame.locator("body").evaluate(() => {
    parent.postMessage({
      wicket: 1,
      type: "submit",
      data: { drafts: [{ id: "northwind", action: "post", subject: "x", body: "y" }], undecided: [] },
    }, "*");
  });
  await expect(page.locator("#gate-violations")).toContainText("/drafts/0/action");
  await expect(frame.locator("#errors")).toContainText("/drafts/0/action");

  // Nothing was decided, so the waiter is still blocking on a pending gate.
  await expect(page.locator("[data-handover]")).toBeVisible();
  waiter.proc.kill();
});
