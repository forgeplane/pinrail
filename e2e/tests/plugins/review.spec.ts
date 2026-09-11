import { expect, test, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { gateUrl, startWaiter, tmpFile, wicketJson } from "../../helpers/wicket";

const reviewPayload = JSON.parse(fs.readFileSync(path.resolve(__dirname, "../../../plugins/review/fixtures/dedup-round-2.json"), "utf8")).payload;

const plugin = (page: Page) => page.frameLocator("#plugin-frame");

function createReviewGate(title: string, extra: string[] = []) {
  const payload = tmpFile("review.json", JSON.stringify(reviewPayload));
  return startWaiter(["create", "review", "--title", title, "--source", "repo=acme,workflow=review,ref=42", "--data", payload, "--wait", ...extra]);
}

test("renders the diff, threads and suggestion, and returns exactly what was decided", async ({ page }) => {
  const waiter = createReviewGate("review: full round");
  const id = await waiter.gateId;
  await page.goto(gateUrl(id));
  const frame = plugin(page);

  await expect(frame.locator("header")).toContainText("Dedup tickets on save");
  await expect(frame.locator("header")).toContainText("!42");
  await expect(frame.locator("aside")).toContainText("FILES · 2");
  await expect(frame.locator('[data-filesec="lib/acme/tickets.ex"]')).toContainText("+3 −2");
  await expect(frame.locator("#card-18")).toContainText("SUGGESTED CHANGE");
  await expect(frame.locator("#card-19")).toContainText("2 comments (1 from the developer)");
  await expect(frame.locator("#card-19")).toContainText("Fair, I added the backfill");
  await expect(frame.locator("#card-20")).toBeVisible(); // unplaceable: its file is not in the diff

  await frame.locator("#card-18 button", { hasText: "Accept" }).click();
  await frame.locator("#card-19 button", { hasText: "Reject" }).click();
  await frame.getByLabel("note for proposal 19").fill("the backfill covers it");
  await frame.getByLabel("note for proposal 19").press("Enter");
  await expect(frame.locator("#card-19")).toContainText("REJECTION REASON");

  await frame.locator('[data-filesec="lib/acme/tickets.ex"] .diff-row').filter({ hasText: "Repo.insert_all" }).hover();
  await frame.getByLabel("comment on lib/acme/tickets.ex:150", { exact: true }).click();
  await frame.getByLabel("your comment").fill("is insert_all chunked anywhere?");
  await frame.locator('[data-act="save-composer"]').click();
  await expect(frame.locator("[data-comment]")).toContainText("is insert_all chunked anywhere?");

  await frame.getByLabel("general comment").fill("Nice change overall.");
  await frame.getByLabel("general comment").press("Enter");
  await expect(frame.locator("[data-general]")).toContainText("Nice change overall.");

  await page.getByLabel("Note to the agent").fill("round 3: only the migration");
  await frame.getByRole("button", { name: "Submit decisions" }).click();
  await expect(frame.locator("#submit-modal")).toContainText("1 proposal(s) still undecided");
  await frame.getByRole("button", { name: /^Submit \d+ decision/ }).click();
  await frame.getByRole("button", { name: /^Confirm — 1 left undecided/ }).click();

  await expect(page.locator("#gate-decision")).toContainText("round 3: only the migration");
  await expect(frame.locator("#done-banner")).toBeVisible();
  await expect(frame.locator('[data-verdict="18"]')).toHaveText("ACCEPTED");
  await expect(frame.locator('[data-verdict="20"]')).toHaveText("UNDECIDED");

  const result = await waiter.done;
  expect(result.code, result.stderr).toBe(0);
  const envelope = JSON.parse(result.stdout);
  expect(envelope.agent_note).toBe("round 3: only the migration");
  expect(envelope.decision.data).toEqual({
    decisions: [
      { id: 18, action: "accept" },
      { id: 19, action: "reject", note: "the backfill covers it" },
    ],
    comments: [{ file: "lib/acme/tickets.ex", line: 150, side: "new", body: "is insert_all chunked anywhere?" }],
    general_comments: [{ body: "Nice change overall." }],
    undecided: [20],
  });
});

test("keyboard flow: j/a/x decide, s opens the summary, Ctrl+Enter confirms; a draft survives reload", async ({ page }) => {
  const waiter = createReviewGate("review: keys");
  const id = await waiter.gateId;
  await page.goto(gateUrl(id));
  const frame = plugin(page);
  await expect(frame.locator("#card-18")).toBeVisible();

  await frame.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("a"); // accept the focused (#18), focus advances to #19
  await expect(frame.locator("#card-18")).toContainText("ACCEPTED");
  await page.keyboard.press("x"); // reject #19, the reason editor opens
  await frame.getByLabel("note for proposal 19").fill("no");
  await page.keyboard.press("Enter");
  await expect(frame.locator("header")).toContainText("2 of 3 decided");

  await page.waitForFunction((key) => (sessionStorage.getItem(key) || "").includes('"19"'), `wicket:draft:${id}`);
  await page.reload();
  await expect(frame.locator("header")).toContainText("2 of 3 decided");
  await expect(frame.locator("#card-19")).toContainText("REJECTED");

  await frame.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("a"); // focus starts on the first undecided: #20
  await page.keyboard.press("s");
  await expect(frame.locator("#submit-modal")).toContainText("Submit 3 decision(s)");
  await page.keyboard.press("Control+Enter");
  await expect(page.locator("#gate-decision")).toBeVisible();

  const result = await waiter.done;
  expect(result.code).toBe(0);
  expect(JSON.parse(result.stdout).decision.data.undecided).toEqual([]);
  expect(JSON.parse(result.stdout).decision.data.decisions.map((d: any) => d.id)).toEqual([18, 19, 20]);
});

test("a superseding round shows the previous verdicts and a withdrawn gate reads as such", async ({ page }) => {
  const r1 = createReviewGate("review: round 1");
  const id1 = await r1.gateId;
  wicketJson(["decide", id1, "--data", tmpFile("d.json", JSON.stringify({ decisions: [{ id: 18, action: "reject", note: "dont nitpick" }], comments: [], undecided: [19, 20] }))]);
  expect((await r1.done).code).toBe(0);

  const r2 = createReviewGate("review: round 2", ["--supersedes", id1]);
  const id2 = await r2.gateId;
  await page.goto(gateUrl(id2));
  const frame = plugin(page);
  await expect(frame.locator("#card-18")).toContainText("PREVIOUS ROUND reject — dont nitpick");
  await expect(frame.locator("#card-19")).toContainText("PREVIOUS ROUND undecided");

  wicketJson(["withdraw", id2]);
  await expect(page.locator("#gate-withdrawn")).toBeVisible();
  await expect(frame.locator("header")).toContainText("READ-ONLY · WITHDRAWN");
  await expect(frame.getByRole("button", { name: "Submit decisions" })).toHaveCount(0);
  expect((await r2.done).code).toBe(3);
});
