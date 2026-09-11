import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "../../../wicket_sdk/testing/playwright";

const dir = path.resolve(__dirname, "..");
const round2 = () => fixture(path.join(dir, "fixtures", "dedup-round-2.json"));
const round1 = () => fixture(path.join(dir, "fixtures", "dedup-round-1.decided.json"));

test("renders the change, the tree, the diff, the anchored cards, a thread and a suggestion", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round2() });
  const f = plugin.frame;
  await expect(f.locator("header")).toContainText("Dedup tickets on save");
  await expect(f.locator("header")).toContainText("!42");
  await expect(f.locator("header")).toContainText("0 of 3 decided");
  await expect(f.locator("aside")).toContainText("FILES · 2");
  await expect(f.locator('[data-filesec="lib/acme/tickets.ex"]')).toContainText("+3 −2");
  await expect(f.locator('[data-filesec="lib/acme/tickets.ex"] .diff-row').filter({ hasText: "Enum.reverse()" })).toHaveCount(3);
  await expect(f.locator("#card-18")).toContainText("SUGGESTED CHANGE");
  await expect(f.locator("#card-18")).toContainText("|> Enum.uniq_by(& &1.id)");
  await expect(f.locator("#card-19")).toContainText("REPLY");
  await expect(f.locator("#card-19")).toContainText("will resolve thread");
  await expect(f.locator("#card-19")).toContainText("2 comments (1 from the developer)");
  await expect(f.locator("#card-19")).toContainText("Fair, I added the backfill");
  await expect(f.locator("#card-20")).toBeVisible();
  const resize = (await plugin.messages()).find((m) => m.type === "resize");
  expect(resize?.height).toBe("fill");
});

test("verdicts, notes, own comments and general comments become exactly the decision", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round2() });
  const f = plugin.frame;
  await f.locator("#card-18 button", { hasText: "Accept" }).click();
  await f.locator("#card-19 button", { hasText: "Reject" }).click();
  await f.getByLabel("note for proposal 19").fill("the backfill covers it");
  await f.getByLabel("note for proposal 19").press("Enter");
  await expect(f.locator("#card-19")).toContainText("REJECTION REASON");

  await f.locator('[data-filesec="lib/acme/tickets.ex"] .diff-row').filter({ hasText: "Repo.insert_all" }).hover();
  await f.getByLabel("comment on lib/acme/tickets.ex:150", { exact: true }).click();
  await f.getByLabel("your comment").fill("is insert_all chunked anywhere?");
  await f.locator('[data-act="save-composer"]').click();
  await expect(f.locator("[data-comment]")).toContainText("is insert_all chunked anywhere?");

  await f.getByLabel("general comment").fill("Nice change overall.");
  await f.getByLabel("general comment").press("Enter");

  await plugin.collect();
  await expect(f.locator("#submit-modal")).toContainText("1 proposal(s) still undecided");
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over");
  await plugin.collect();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over, 1 undecided");
  await plugin.collect();

  expect(await plugin.nextSubmit()).toEqual({
    decisions: [
      { id: 18, action: "accept" },
      { id: 19, action: "reject", note: "the backfill covers it" },
    ],
    comments: [{ file: "lib/acme/tickets.ex", line: 150, side: "new", body: "is insert_all chunked anywhere?" }],
    general_comments: [{ body: "Nice change overall." }],
    undecided: [20],
  });
});

test("keyboard: a / x / j decide and move, s opens the summary, collect confirms", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round2() });
  const f = plugin.frame;
  await expect(f.locator("#card-18")).toBeVisible();
  await expect.poll(() => plugin.lastStatus()).toBe("Review and hand over");
  await f.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("a");
  await expect(f.locator("#card-18")).toContainText("ACCEPTED");
  await page.keyboard.press("x");
  await f.getByLabel("note for proposal 19").fill("no");
  await page.keyboard.press("Enter");
  await page.keyboard.press("a");
  await expect(f.locator("header")).toContainText("3 of 3 decided");
  await page.keyboard.press("s");
  await expect(f.locator("#submit-modal")).toContainText("Hand over 3 decision(s)");
  await plugin.collect();
  const data = await plugin.nextSubmit();
  expect(data.undecided).toEqual([]);
  expect(data.decisions.map((d: any) => [d.id, d.action, d.note])).toEqual([[18, "accept", undefined], [19, "reject", "no"], [20, "accept", undefined]]);
});

test("a draft survives a reload", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round2() });
  const f = plugin.frame;
  await f.locator("#card-18 button", { hasText: "Accept" }).click();
  await f.locator("#card-19 button", { hasText: "Reject" }).click();
  await f.getByLabel("note for proposal 19").fill("later");
  await page.keyboard.press("Enter");
  await expect.poll(() => plugin.lastDraft().then((d) => d && d.decisions && d.decisions["19"] && d.decisions["19"].note)).toBe("later");
  await plugin.reload();
  await plugin.reinit();
  await expect(f.locator("header")).toContainText("2 of 3 decided");
  await expect(f.locator("#card-19")).toContainText("REJECTED");
  await expect(f.locator("#card-19")).toContainText("later");
});

test("violations reopen the summary with the errors; submitted renders read-only with verdicts", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round2() });
  const f = plugin.frame;
  await plugin.sendViolations([{ path: "/comments/0/line", message: "value is not of type integer" }]);
  await expect(f.locator("#submit-modal #errors")).toContainText("/comments/0/line: value is not of type integer");
  await f.getByRole("button", { name: "Keep reviewing" }).click();

  await plugin.sendSubmitted({ decided_by: "alice", decided_at: "2026-09-11T10:00:00Z", data: { decisions: [{ id: 18, action: "accept" }], comments: [], undecided: [19, 20] } });
  await expect(f.locator("#done-banner")).toBeVisible();
  await expect(f.locator("header")).toContainText("READ-ONLY · DECIDED");
  await expect(f.locator('[data-verdict="18"]')).toHaveText("ACCEPTED");
  await expect(f.locator('[data-verdict="19"]')).toHaveText("UNDECIDED");
  await expect(f.getByRole("button", { name: "Accept" })).toHaveCount(0);
});

test("a superseding gate shows the previous round's verdicts; withdrawn reads as read-only", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round2(), previous: round1() });
  const f = plugin.frame;
  await expect(f.locator("#card-18")).toContainText("PREVIOUS ROUND reject — dont nitpick");
  await expect(f.locator("#card-19")).toContainText("PREVIOUS ROUND undecided");

  const withdrawn = await mountPlugin(page, dir, { gate: { ...round2(), status: "withdrawn" }, readonly: true });
  await expect(withdrawn.frame.locator("header")).toContainText("READ-ONLY · WITHDRAWN");
  await expect(withdrawn.frame.locator("#submit-modal")).toHaveCount(0);
});

test("the header leaves out what the shell's own header already shows", async ({ page }) => {
  const change = round2().payload.change;

  // A gate whose source names the same change: the shell puts the ref and a
  // link to it above the frame, so the view's header carries neither.
  const shared = await mountPlugin(page, dir, {
    gate: { ...round2(), source: { repo: "acme", workflow: "mr-review", ref: "42", url: change.url } },
  });
  const header = shared.frame.locator("header").first();
  await expect(header).toContainText("Dedup tickets on save");
  await expect(header).toContainText("fix/tickets-dedup");
  await expect(header).not.toContainText("!42");
  await expect(header.locator(`a[href="${change.url}"]`)).toHaveCount(0);

  // A gate whose source says nothing about it: the view keeps both, because
  // nothing else on the page is showing them.
  const alone = await mountPlugin(page, dir, {
    gate: { ...round2(), source: { repo: "acme", workflow: "nightly" } },
  });
  const own = alone.frame.locator("header").first();
  await expect(own).toContainText("!42");
  await expect(own.locator(`a[href="${change.url}"]`)).toHaveCount(1);
});
