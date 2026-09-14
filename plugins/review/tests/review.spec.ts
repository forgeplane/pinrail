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

test("the toolbar carries icons, and the control with no words carries its name", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round2() });
  const header = plugin.frame.locator("header").first();

  const icons = await header.locator(".wi").evaluateAll((els) => els.map((e) => (e as HTMLElement).dataset.icon));
  expect(icons).toEqual(["panel-left-close", "rows-3", "columns-2", "message-square", "fold-vertical", "list-check", "list-x"]);

  // Beside a word, an icon is decoration and the word is the name.
  await expect(header.getByRole("button", { name: "Accept remaining (3)" })).toBeVisible();
  await expect(header.getByRole("button", { name: "Fold all" })).toBeVisible();
  await expect(header.locator('[data-act="bulk-accept"] .wi')).toHaveAttribute("aria-hidden", "true");

  // The diff toggle has no words, so its two buttons are named.
  await expect(header.getByRole("button", { name: "Inline diff" })).toBeVisible();
  await expect(header.getByRole("button", { name: "Split diff" })).toBeVisible();

  // Folding flips the label and the icon together.
  await header.getByRole("button", { name: "Fold all" }).click();
  await expect(header.getByRole("button", { name: "Unfold all" })).toBeVisible();
  await expect(header.locator('[data-act="fold-all"] .wi')).toHaveAttribute("data-icon", "unfold-vertical");

  // Confirming a bulk action changes the words, not the icon.
  await header.getByRole("button", { name: "Reject remaining (3)" }).click();
  await expect(header.getByRole("button", { name: "Really reject 3?" })).toBeVisible();
  await expect(header.locator('[data-act="bulk-reject"] .wi')).toHaveAttribute("data-icon", "list-x");
});

test("the file tree's controls carry icons, and the collapse in the header says which way it goes", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round2() });
  const aside = plugin.frame.locator("aside");

  await expect(aside.locator(".wi")).toHaveCount(2);
  await expect(aside.getByRole("button", { name: "semantic" })).toBeVisible();
  await expect(aside.getByRole("button", { name: "findings" })).toBeVisible();

  // The order pill's icon says which order is on, along with its label.
  const order = plugin.frame.locator('[data-act="toggle-order"]');
  await expect(order.locator(".wi")).toHaveAttribute("data-icon", "list-ordered");
  await order.click();
  await expect(order).toHaveText("a→z");
  await expect(order.locator(".wi")).toHaveAttribute("data-icon", "arrow-down-a-z");

  // The collapse has no words, so it is named, and the name follows the state.
  const collapse = plugin.frame.locator('[data-act="toggle-tree"]');
  await expect(collapse.locator(".wi")).toHaveAttribute("data-icon", "panel-left-close");
  await expect(plugin.frame.getByRole("button", { name: "Collapse the file tree" })).toBeVisible();

  await collapse.click();
  await expect(plugin.frame.locator("aside")).toHaveCount(0);
  await expect(plugin.frame.getByRole("button", { name: "Show the file tree" })).toBeVisible();
  await expect(plugin.frame.locator('[data-act="toggle-tree"] .wi')).toHaveAttribute("data-icon", "panel-left-open");

  await plugin.frame.locator('[data-act="toggle-tree"]').click();
  await expect(plugin.frame.locator("aside")).toBeVisible();
});

test("settings lay out the view; a pill or a key asks the shell to keep the choice", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round2(), settings: { diff: "split", order: "path", findings_only: true, tree_open: false } });
  const f = plugin.frame;
  const splitRows = f.locator(".diff-row.split");
  await expect(f.locator("#card-18")).toBeVisible();
  await expect(splitRows.first()).toBeVisible();
  await expect(f.locator("aside")).toHaveCount(0);

  // the app's Settings changed: the view follows without a re-init
  await plugin.settings({ diff: "inline", order: "path", findings_only: false, tree_open: true });
  await expect(splitRows).toHaveCount(0);
  await expect(f.locator("aside")).toContainText("FILES · 2");

  // the pill and the key go through the shell, and the view shows the choice at once
  await f.locator('[data-act="set-split"]').click();
  await expect.poll(() => plugin.lastSettingsSet()).toEqual({ diff: "split" });
  await expect(splitRows.first()).toBeVisible();
  await f.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("v");
  await expect.poll(() => plugin.lastSettingsSet()).toEqual({ diff: "inline" });
  await f.locator('[data-act="toggle-findings-only"]').click();
  await expect.poll(() => plugin.lastSettingsSet()).toEqual({ findings_only: true });
});

test("a declared key forwarded by the shell works like one typed in the frame", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round2() });
  const f = plugin.frame;
  await expect(f.locator("#card-18")).toBeVisible();
  await plugin.sendKey("j");
  await expect(f.locator("#card-19")).toHaveAttribute("style", /inset 3px 0 0 var\(--accent\)/);
  await plugin.sendKey("k");
  await expect(f.locator("#card-18")).toHaveAttribute("style", /inset 3px 0 0 var\(--accent\)/);
  await plugin.sendKey("a");
  await expect(f.locator("#card-18")).toContainText("ACCEPTED");
  await plugin.sendKey("v");
  await expect(f.locator(".diff-row.split").first()).toBeVisible();
});

test("the brief is a strip under the header; details drop down; the comments sit after the last file", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round2() });
  const f = plugin.frame;
  const brief = f.locator("#brief");
  await expect(brief).toContainText("Dedups tickets before the bulk insert");
  await expect(brief).not.toContainText("REVIEW CONCERNS");
  // the chip opens the concerns alone; Details opens the description; the summary line is never repeated
  await brief.getByRole("button", { name: "concerns" }).click();
  await expect(brief).toContainText("Ordering of the dedup");
  await expect(brief).not.toContainText("DESCRIPTION");
  await brief.getByRole("button", { name: "Details" }).click();
  await expect(brief).toContainText("DESCRIPTION");
  await expect(brief).toContainText("Closes #12");
  expect((await brief.innerText()).split("Dedups tickets before the bulk insert").length).toBe(2);
  await brief.getByRole("button", { name: "Hide details" }).click();
  await brief.getByRole("button", { name: "concerns" }).click();
  await expect(brief).not.toContainText("Ordering of the dedup");
  // the comments come after the files, and the header's button lands on them
  const general = f.locator("#general-comments");
  const last = f.locator("[data-filesec]").last();
  expect((await general.boundingBox())!.y).toBeGreaterThan((await last.boundingBox())!.y);
  await f.locator('[data-act="jump-general"]').click();
  await expect(f.getByLabel("general comment")).toBeFocused();
  await f.getByLabel("general comment").fill("Overall fine.");
  await f.getByLabel("general comment").press("Enter");
  await expect(f.locator('[data-act="jump-general"]')).toContainText("1");
});

