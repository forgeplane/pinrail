import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin, reviewFrom } from "pinrail-sdk/testing";

const dir = path.resolve(__dirname, "..");
const round2 = () => fixture(path.join(dir, "fixtures", "dedup-round-2.json"));
const round1 = () => fixture(path.join(dir, "fixtures", "dedup-round-1.decided.json"));

test("renders the change, the tree, the diff, the anchored cards, a thread and a suggestion", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  await expect(f.locator("header")).toContainText("Dedup tickets on save");
  await expect(f.locator("header")).toContainText("!42");
  await expect(f.locator("header")).toContainText("3 undecided");
  await expect(f.locator("aside")).toContainText("FILES · 2");
  await expect(f.locator('[data-filesec="lib/acme/tickets.ex"]')).toContainText("+3 −2");
  await expect(
    f.locator('[data-filesec="lib/acme/tickets.ex"] .diff-row').filter({ hasText: "Enum.reverse()" }),
  ).toHaveCount(3);
  await expect(f.locator("#card-18")).toContainText("SUGGESTED CHANGE");
  await expect(f.locator("#card-18")).toContainText("|> Enum.uniq_by(& &1.id)");
  await expect(f.locator("#card-19")).toContainText("REPLY");
  await expect(f.locator("#card-19")).toContainText("will resolve thread");
  await expect(f.locator("#card-19")).toContainText("2 comments (1 from the developer)");
  await expect(f.locator("#card-19")).toContainText("Fair, I added the backfill");
  await expect(f.locator("#card-20")).toBeVisible();
});

test("verdicts, notes and own comments become exactly the decision", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
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

  // with one left undecided, the first hand-over asks in the confirmation bar
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over");
  await plugin.collect();
  const bar = f.locator(".pinrail-confirmation");
  await expect(bar).toHaveClass(/pinrail-confirmation-warning/);
  await expect(bar).toContainText("1 proposal left undecided");
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over with 1 undecided");
  expect((await plugin.messages()).filter((m: any) => m.type === "submit")).toHaveLength(0);
  await plugin.collect();

  expect(await plugin.nextSubmit()).toEqual({
    decisions: [
      { id: 18, action: "accept" },
      { id: 19, action: "reject", note: "the backfill covers it" },
    ],
    comments: [{ file: "lib/acme/tickets.ex", line: 150, side: "new", body: "is insert_all chunked anywhere?" }],
    undecided: [20],
  });
});

test("a saved note and your comment have edit and delete at the right of their header", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  await f.locator("#card-19 button", { hasText: "Reject" }).click();
  await f.getByLabel("note for proposal 19").fill("the backfill covers it");
  await f.getByLabel("note for proposal 19").press("Enter");
  const note = f.locator("#card-19 .note.saved");
  await expect(note.locator(".note-head")).toContainText("REJECTION REASON");
  await note.getByRole("button", { name: "Edit the note" }).click();
  await expect(f.getByLabel("note for proposal 19")).toHaveValue("the backfill covers it");
  await f.getByLabel("note for proposal 19").press("Enter");
  await f.locator("#card-19 .note.saved").getByRole("button", { name: "Delete the note" }).click();
  await expect(f.locator("#card-19 .note.saved")).toHaveCount(0);

  await f.locator('[data-filesec="lib/acme/tickets.ex"] .diff-row').filter({ hasText: "Repo.insert_all" }).hover();
  await f.getByLabel("comment on lib/acme/tickets.ex:150", { exact: true }).click();
  await f.getByLabel("your comment").fill("is insert_all chunked anywhere?");
  await f.getByLabel("your comment").press("Enter");
  const mine = f.locator("[data-comment]");
  await mine.locator(".mine-head").getByRole("button", { name: "Edit" }).click();
  await expect(f.getByLabel("your comment")).toHaveValue("is insert_all chunked anywhere?");
  await f.getByLabel("your comment").press("Escape");
  await mine.locator(".mine-head").getByRole("button", { name: "Delete" }).click();
  await expect(f.locator("[data-comment]")).toHaveCount(0);
});

test("keyboard: a / x / j decide and move; with nothing undecided, a hand-over goes at once", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  await expect(f.locator("#card-18")).toBeVisible();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over");
  await f.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("a");
  await expect(f.locator("#card-18")).toContainText("ACCEPTED");
  await page.keyboard.press("x");
  await f.getByLabel("note for proposal 19").fill("no");
  await page.keyboard.press("Enter");
  // saving the reason keeps the focus on #19, and n moves to the next one
  await expect(f.locator("#card-19")).toHaveClass(/\bfocused\b/);
  await page.keyboard.press("n");
  await page.keyboard.press("a");
  await expect(f.locator("header")).toContainText("0 undecided");
  await plugin.collect();
  await expect(f.locator(".pinrail-confirmation")).toBeHidden();
  const data = await plugin.nextSubmit();
  expect(data.undecided).toEqual([]);
  expect(data.decisions.map((d: any) => [d.id, d.action, d.note])).toEqual([
    [18, "accept", undefined],
    [19, "reject", "no"],
    [20, "accept", undefined],
  ]);
});

test("n and p go to the undecided proposals; shift+n and shift+p go through all", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  await f.locator("#card-19 button", { hasText: "Accept" }).click();
  await f.locator("body").click({ position: { x: 5, y: 5 } });
  const focused = (id: number) => expect(f.locator(`#card-${id}`)).toHaveClass(/\bfocused\b/);

  // deciding #19 left the focus on it
  await focused(19);
  await page.keyboard.press("n");
  await focused(20);
  await page.keyboard.press("n");
  await focused(18); // round again, past #19, which is decided
  await page.keyboard.press("p");
  await focused(20);
  await page.keyboard.press("Shift+P");
  await focused(19);
  await page.keyboard.press("Shift+N");
  await focused(20);

  // with no undecided proposals left, n moves through all of them
  await f.locator("#card-18 button", { hasText: "Accept" }).click();
  await f.locator("#card-20 button", { hasText: "Accept" }).click();
  await f.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("n");
  await focused(18);
  await page.keyboard.press("n");
  await focused(19);
});

test("a draft survives a reload", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  await f.locator("#card-18 button", { hasText: "Accept" }).click();
  await f.locator("#card-19 button", { hasText: "Reject" }).click();
  await f.getByLabel("note for proposal 19").fill("later");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => plugin.lastDraft().then((d) => d && d.decisions && d.decisions["19"] && d.decisions["19"].note))
    .toBe("later");
  await plugin.reload();
  await plugin.reinit();
  await expect(f.locator("header")).toContainText("1 undecided");
  await expect(f.locator("#card-19")).toContainText("REJECTED");
  await expect(f.locator("#card-19")).toContainText("later");
});

test("violations show in the confirmation bar; submitted renders read-only with verdicts", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  await expect(f.locator("#card-18")).toBeVisible();
  // hand over: the confirmation with undecided left, then the submit
  await plugin.collect();
  const bar = f.locator(".pinrail-confirmation");
  await expect(bar).toContainText("3 proposals left undecided");
  // keeping on reviewing takes the question back
  await bar.getByRole("button", { name: "Keep reviewing" }).click();
  await expect(bar).toBeHidden();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over");
  await plugin.collect();
  await plugin.collect();
  await plugin.nextSubmit();
  await plugin.sendViolations([{ path: "/comments/0/line", message: "value is not of type integer" }]);
  await expect(bar).toHaveClass(/pinrail-confirmation-danger/);
  await expect(bar).toContainText("/comments/0/line: value is not of type integer");
  await bar.getByRole("button", { name: "Keep reviewing" }).click();
  await expect(bar).toBeHidden();

  await plugin.sendSubmitted({
    decided_by: "alice",
    decided_at: "2026-09-11T10:00:00Z",
    data: { decisions: [{ id: 18, action: "accept" }], comments: [], undecided: [19, 20] },
  });
  await expect(f.locator("#done-banner")).toBeVisible();
  await expect(f.locator("header")).toContainText("READ-ONLY · DECIDED");
  await expect(f.locator('[data-verdict="18"]')).toHaveText("ACCEPTED");
  await expect(f.locator('[data-verdict="19"]')).toHaveText("UNDECIDED");
  await expect(f.getByRole("button", { name: "Accept" })).toHaveCount(0);
});

test("a superseding review shows the previous round's verdicts; withdrawn reads as read-only", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2(), previous: round1() });
  const f = plugin.frame;
  await expect(f.locator("#card-18")).toContainText("PREVIOUS ROUND reject — dont nitpick");
  await expect(f.locator("#card-19")).toContainText("PREVIOUS ROUND undecided");

  const withdrawn = await mountPlugin(page, dir, { review: { ...round2(), status: "withdrawn" }, readonly: true });
  await expect(withdrawn.frame.locator("header")).toContainText("READ-ONLY · WITHDRAWN");
  await expect(withdrawn.frame.locator(".pinrail-confirmation")).toBeHidden();
});

test("the header names the change, with a link to it", async ({ page }) => {
  const change = round2().payload.change;
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const header = plugin.frame.locator("header").first();
  await expect(header).toContainText("Dedup tickets on save");
  await expect(header).toContainText("fix/tickets-dedup");
  await expect(header).toContainText("!42");
  await expect(header.locator(`a[href="${change.url}"]`)).toHaveCount(1);
});

test("the diff's bar holds how it reads and the bulk decisions; icons carry names only where there are no words", async ({
  page,
}) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const header = plugin.frame.locator("header").first();
  const bar = plugin.frame.locator('[role="toolbar"]');

  // the header: only the rail's toggle
  expect(
    await header.locator(".pinrail-icon").evaluateAll((els) => els.map((e) => (e as HTMLElement).dataset.icon)),
  ).toEqual(["panel-left-close"]);
  // the diff's bar: how it reads, and the decisions on the findings still open below
  expect(
    await bar.locator(".pinrail-icon").evaluateAll((els) => els.map((e) => (e as HTMLElement).dataset.icon)),
  ).toEqual([
    "rows-3",
    "columns-2",
    "fold-vertical",
    "wrap-text",
    "chevron-left",
    "chevron-right",
    "list-check",
    "list-x",
  ]);

  // Beside a word, an icon is decoration and the word is the name.
  await expect(bar.getByRole("button", { name: "Accept remaining (3)" })).toBeVisible();
  await expect(bar.getByRole("button", { name: "Fold all" })).toBeVisible();
  await expect(bar.locator('[data-act="bulk-accept"] .pinrail-icon')).toHaveAttribute("aria-hidden", "true");

  // The arrow buttons for moving between proposals have names too.
  await expect(bar.getByRole("button", { name: "Next undecided proposal" })).toBeVisible();

  // The diff toggle has no words, so its two buttons are named.
  await expect(bar.getByRole("button", { name: "Inline diff" })).toBeVisible();
  await expect(bar.getByRole("button", { name: "Split diff" })).toBeVisible();

  // Folding flips the label and the icon together.
  await bar.getByRole("button", { name: "Fold all" }).click();
  await expect(bar.getByRole("button", { name: "Unfold all" })).toBeVisible();
  await expect(bar.locator('[data-act="fold-all"] .pinrail-icon')).toHaveAttribute("data-icon", "unfold-vertical");

  // Confirming a bulk action changes the words, not the icon.
  await bar.getByRole("button", { name: "Reject remaining (3)" }).click();
  await expect(bar.getByRole("button", { name: "Really reject 3?" })).toBeVisible();
  await expect(bar.locator('[data-act="bulk-reject"] .pinrail-icon')).toHaveAttribute("data-icon", "list-x");
});

test("the file tree's controls carry icons, and the collapse in the header says which way it goes", async ({
  page,
}) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const aside = plugin.frame.locator("aside");

  await expect(aside.locator(".rail-head .pinrail-icon")).toHaveCount(2);
  // each folder opens and closes on a chevron, beside a folder that says which
  await expect(aside.locator('[data-act="toggle-dir"] .pinrail-icon[data-icon="folder-open"]')).toHaveCount(2);
  await expect(aside.getByRole("button", { name: "semantic" })).toBeVisible();
  await expect(aside.getByRole("button", { name: "findings" })).toBeVisible();

  // The order pill's icon says which order is on, along with its label.
  const order = plugin.frame.locator('[data-act="toggle-order"]');
  await expect(order.locator(".pinrail-icon")).toHaveAttribute("data-icon", "list-ordered");
  await order.click();
  await expect(order).toHaveText("a→z");
  await expect(order.locator(".pinrail-icon")).toHaveAttribute("data-icon", "arrow-down-a-z");

  // The collapse has no words, so it is named, and the name follows the state.
  const collapse = plugin.frame.locator('[data-act="toggle-tree"]');
  await expect(collapse.locator(".pinrail-icon")).toHaveAttribute("data-icon", "panel-left-close");
  await expect(plugin.frame.getByRole("button", { name: "Collapse the file tree" })).toBeVisible();

  await collapse.click();
  await expect(plugin.frame.locator("aside")).toHaveCount(0);
  await expect(plugin.frame.getByRole("button", { name: "Show the file tree" })).toBeVisible();
  await expect(plugin.frame.locator('[data-act="toggle-tree"] .pinrail-icon')).toHaveAttribute(
    "data-icon",
    "panel-left-open",
  );

  await plugin.frame.locator('[data-act="toggle-tree"]').click();
  await expect(plugin.frame.locator("aside")).toBeVisible();
});

test("settings lay out the view; a pill or a key asks the shell to keep the choice", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, {
    review: round2(),
    settings: { diff: "split", order: "path", findings_only: true, tree_open: false },
  });
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

  // wrapping: a setting, a button and a key; unwrapped, a file scrolls sideways
  await plugin.settings({ diff: "inline", order: "path", findings_only: false, tree_open: true, wrap: false });
  await expect(f.locator(".file-diff.nowrap").first()).toBeVisible();
  // the + still shows over the pinned gutter
  const line = f.locator('[data-filesec="lib/acme/tickets.ex"] .diff-row').filter({ hasText: "Repo.insert_all" });
  await line.hover();
  const plus = f.getByLabel("comment on lib/acme/tickets.ex:150", { exact: true });
  const box = (await plus.boundingBox())!;
  const top = await f
    .locator("body")
    .evaluate(
      (_, [x, y]) => document.elementFromPoint(x, y)?.closest("#addbtn") != null,
      [box.x + box.width / 2, box.y + box.height / 2],
    );
  expect(top).toBe(true);
  // and on a deleted line, commented on its old number
  const gone = f.locator("[data-filesec] .diff-row.inline.del").first();
  await gone.hover();
  const oldPlus = f.locator("#addbtn");
  await expect(oldPlus).toHaveAttribute("data-side", "old");
  const ob = (await oldPlus.boundingBox())!;
  const onTop = await f
    .locator("body")
    .evaluate(
      (_, [x, y]) => document.elementFromPoint(x, y)?.closest("#addbtn") != null,
      [ob.x + ob.width / 2, ob.y + ob.height / 2],
    );
  expect(onTop).toBe(true);
  await f.locator('[data-act="toggle-wrap"]').click();
  await expect.poll(() => plugin.lastSettingsSet()).toEqual({ wrap: true });
  await expect(f.locator(".file-diff.nowrap")).toHaveCount(0);
  await f.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("w");
  await expect.poll(() => plugin.lastSettingsSet()).toEqual({ wrap: false });
  // side by side always wraps
  await f.locator('[data-act="set-split"]').click();
  await expect(f.locator('[data-act="toggle-wrap"]')).toBeDisabled();
  await expect(f.locator(".file-diff.nowrap")).toHaveCount(0);
});

test("a declared key forwarded by the shell works like one typed in the frame", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  await expect(f.locator("#card-18")).toBeVisible();
  await plugin.sendKey("n");
  await expect(f.locator("#card-19")).toHaveClass(/\bfocused\b/);
  await plugin.sendKey("p");
  await expect(f.locator("#card-18")).toHaveClass(/\bfocused\b/);
  await plugin.sendKey("a");
  await expect(f.locator("#card-18")).toContainText("ACCEPTED");
  await plugin.sendKey("v");
  await expect(f.locator(".diff-row.split").first()).toBeVisible();
});

test("the brief opens the scroll; concerns and the description drop down in it", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  const brief = f.locator("#brief");
  await expect(brief).toContainText("Dedups tickets before the bulk insert");
  await expect(brief).not.toContainText("REVIEW CONCERNS");
  // the chip opens the concerns alone; Details opens the description; the summary line is never repeated
  await brief.getByRole("button", { name: "concerns" }).click();
  await expect(brief).toContainText("Ordering of the dedup");
  await expect(brief).not.toContainText("DESCRIPTION");
  await brief.getByRole("button", { name: "description" }).click();
  await expect(brief).toContainText("DESCRIPTION");
  await expect(brief).toContainText("Closes #12");
  expect((await brief.innerText()).split("Dedups tickets before the bulk insert").length).toBe(2);
  await brief.getByRole("button", { name: "description" }).click();
  await expect(brief).not.toContainText("Closes #12");
  await brief.getByRole("button", { name: "concerns" }).click();
  await expect(brief).not.toContainText("Ordering of the dedup");
  // nothing but the files follows: a note for the agent goes with the hand-over
  await expect(f.locator("#general-comments")).toHaveCount(0);
});

test("the rail lists each file's findings, where they stand, and jumps to one", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  const rail = f.locator("aside");
  const findings = rail.locator('[data-act="jump-card"]');
  // every finding, those on no file under a group of their own at the end
  await expect(findings).toHaveCount(await f.locator("[data-card]").count());
  await expect(rail.locator(".tree-loose")).toContainText("Not on a file");
  await expect(rail.locator('.tree-loose ~ [data-act="jump-card"][data-id="20"]')).toHaveCount(1);
  const first = findings.first();
  const id = await first.getAttribute("data-id");
  await first.click();
  await expect(first).toHaveClass(/focus/);
  await f.locator(`#card-${id} button`, { hasText: "Accept" }).click();
  await expect(rail.locator(`[data-act="jump-card"][data-id="${id}"]`)).toHaveClass(/accepted/);
});

test("a note folds with its finding, writing one opens it, and the rail marks it", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  const card = f.locator("#card-18");
  await card.locator("button", { hasText: "Reject" }).click();
  await f.getByLabel("note for proposal 18").fill("out of scope");
  await f.getByLabel("note for proposal 18").press("Enter");
  // just written, so still in view
  await expect(card).toContainText("out of scope");
  await expect(f.locator('aside [data-act="jump-card"][data-id="18"] .has-note')).toBeVisible();
  // folded, the note goes with the comment
  await card.locator('[data-act="collapse"]').click();
  await expect(card).toHaveClass(/\bfolded\b/);
  await expect(card).not.toContainText("out of scope");
  // asking to edit it opens the finding again
  await card.locator('[data-act="open-note"]').click();
  await expect(f.getByLabel("note for proposal 18")).toHaveValue("out of scope");
  await expect(card).not.toHaveClass(/\bfolded\b/);
});

test("your comment is edited where it stands, by clicking its text", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  await f.locator('[data-filesec="lib/acme/tickets.ex"] .diff-row').filter({ hasText: "Repo.insert_all" }).hover();
  await f.getByLabel("comment on lib/acme/tickets.ex:150", { exact: true }).click();
  await f.getByLabel("your comment").fill("is insert_all chunked?");
  await f.getByLabel("your comment").press("Enter");
  await f.locator("[data-comment]").getByText("is insert_all chunked?").click();
  // one box: the comment itself, now a field, and no second one below it
  await expect(f.locator("[data-comment]")).toHaveCount(0);
  await expect(f.getByLabel("your comment")).toHaveValue("is insert_all chunked?");
  await f.getByLabel("your comment").fill("is insert_all chunked anywhere?");
  await f.getByLabel("your comment").press("Enter");
  await expect(f.locator("[data-comment]")).toHaveCount(1);
  await expect(f.locator("[data-comment]")).toContainText("is insert_all chunked anywhere?");
  // the rail lists it under its file, among the findings by line
  const rail = f.locator("aside");
  const mine = rail.locator('[data-act="jump-comment"]');
  await expect(mine).toContainText("is insert_all chunked anywhere?");
  await expect(mine).toContainText("L150");
  const order = await rail
    .locator('[data-act="jump-card"], [data-act="jump-comment"]')
    .evaluateAll((els) => els.map((e) => e.getAttribute("data-act")));
  expect(order.indexOf("jump-comment")).toBeGreaterThan(0);
});

test("violations that answer no hand-over, such as a refused setting, open nothing", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  await expect(f.locator("#card-18")).toBeVisible();
  await plugin.sendViolations([{ path: "/wrap", message: "not a setting of this plugin" }]);
  await page.waitForTimeout(200);
  await expect(f.locator(".pinrail-confirmation")).toBeHidden();
});

test("expanding a finding keeps it in place, even with a note left open further up", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  const webhooks = fixture(path.join(dir, "fixtures", "webhooks.json"));
  const plugin = await mountPlugin(page, dir, { review: webhooks });
  await plugin.setFrameHeight(800);
  const f = plugin.frame;
  await f.locator("#card-5 .accept-btn").click();
  await f.locator("#card-1 .reject-btn").click(); // its reason left open, unsaved
  const card = f.locator("#card-5");
  await card.evaluate((el) => el.scrollIntoView({ block: "center" }));
  const before = (await card.locator(".card-head").boundingBox())!.y;
  await card.locator('[data-act="expand"]').click();
  await expect(card).not.toHaveClass(/\bfolded\b/);
  expect(Math.abs((await card.locator(".card-head").boundingBox())!.y - before)).toBeLessThan(2);
});

test("a clicked verdict stays on its finding, so c writes its note", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  await f.locator("#card-18 .accept-btn").click();
  await expect(f.locator("#card-18")).toHaveClass(/\bfocused\b/);
  await f.locator("body").click({ position: { x: 5, y: 5 } });
  await expect(f.locator("#card-18")).toHaveClass(/\bfolded\b/);
  await page.keyboard.press("c");
  // c writes the finding's note: it opens, the note field in it
  await expect(f.getByLabel("note for proposal 18")).toBeFocused();
  await expect(f.locator("#card-18")).not.toHaveClass(/\bfolded\b/);
  await page.keyboard.press("Escape");
  await expect(f.getByLabel("note for proposal 18")).toHaveCount(0);
  await expect(f.locator("#card-18")).toHaveClass(/\bfocused\b/);
});

test("clicking a verdict, c and Esc keep the finding where it is on screen", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  const webhooks = fixture(path.join(dir, "fixtures", "webhooks.json"));
  const plugin = await mountPlugin(page, dir, { review: webhooks });
  await plugin.setFrameHeight(800);
  const f = plugin.frame;
  // the last finding below, the rest undecided above it
  const card = f.locator("#card-5");
  await card.evaluate((el) => el.scrollIntoView({ block: "center" }));
  const y = async () => (await card.locator(".card-head").boundingBox())!.y;
  const start = await y();
  await card.locator(".accept-btn").click();
  expect(Math.abs((await y()) - start)).toBeLessThan(2);
  await f.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("c");
  await expect(f.getByLabel("note for proposal 5")).toBeVisible();
  expect(Math.abs((await y()) - start)).toBeLessThan(2);
  await page.keyboard.press("Escape");
  await expect(f.getByLabel("note for proposal 5")).toHaveCount(0);
  expect(Math.abs((await y()) - start)).toBeLessThan(2);
});

test("with the findings filter on, a folder with no file left is not shown", async ({ page }) => {
  // a file with nothing to decide, alone in its folder
  const review = round2();
  review.payload.files.push({ path: "docs/notes.md", status: "modified", diff: "@@ -1 +1 @@\n-old\n+new\n" });
  const plugin = await mountPlugin(page, dir, { review, settings: { findings_only: true } });
  const aside = plugin.frame.locator("aside");
  await expect(aside.locator('[data-act="toggle-dir"][data-dir="docs"]')).toHaveCount(0);
  await expect(aside.locator('[data-act="jump-file"]').first()).toBeVisible();
  const dirs = await aside
    .locator('[data-act="toggle-dir"]')
    .evaluateAll((els) => els.map((e) => e.getAttribute("data-dir")));
  const files = await aside
    .locator('[data-act="jump-file"]')
    .evaluateAll((els) => els.map((e) => e.getAttribute("data-file")));
  for (const d of dirs) expect(files.some((f) => f!.startsWith(d + "/"))).toBe(true);
});

test("in a decided review, c does nothing: no note, and the diff keeps its layout", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round1(), readonly: true });
  const f = plugin.frame;
  await expect(f.locator(".diff-row.inline").first()).toBeVisible();
  await f.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("c");
  await page.waitForTimeout(200);
  await expect(f.locator(".diff-row.split")).toHaveCount(0);
  await expect(f.locator("[data-note-ta]")).toHaveCount(0);
});

test("a proposal's markdown renders in full: lists, links and emphasis", async ({ page }) => {
  // the schema tells agents these fields are markdown
  const review = round2();
  const payload = review.payload as { proposals: { body: string }[] };
  payload.proposals[0].body =
    "Two things:\n\n- drop the second `reverse`\n- keep _one_ pass\n\nSee [the docs](https://hexdocs.pm/elixir/Enum.html).";
  const plugin = await mountPlugin(page, dir, { review });
  const card = plugin.frame.locator(".card-body").first();
  await expect(card.locator("ul li")).toHaveCount(2);
  await expect(card.locator("em")).toHaveText("one");
  await expect(card.locator("a")).toHaveAttribute("href", "https://hexdocs.pm/elixir/Enum.html");
  await expect(card.locator("code").first()).toHaveText("reverse");
});

test("the diff, suggestions and code blocks are highlighted in the file's language", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  const tickets = f.locator('[data-filesec="lib/acme/tickets.ex"]');
  const row = (text: string) => tickets.locator(".diff-row").filter({ hasText: text });
  // a context row, a deleted one and an added one each have their tokens
  await expect(row("def do_save(items) do").locator(".hljs-keyword").first()).toHaveText("def");
  await expect(row("Enum.uniq_by").first().locator(".hljs-title, .hljs-keyword, .hljs-symbol").first()).toBeVisible();
  // the text reads as before
  await expect(row("Repo.insert_all()")).toContainText("|> Repo.insert_all()");
  // the suggested change, in the language of the file it is on
  await expect(f.locator("#card-18 .sugg-row.add .code [class^='hljs-']").first()).toBeVisible();

  await f.getByRole("button", { name: /split/i }).first().click();
  await expect(row("def do_save(items) do").locator(".hljs-keyword").first()).toHaveText("def");
});

test("a string over several lines reads as one, and a file of no known language stays plain", async ({ page }) => {
  const review = reviewFrom({
    title: "Highlighting",
    payload: {
      change: { ref: "!7", title: "Docs" },
      files: [
        {
          path: "lib/acme/docs.ex",
          status: "modified",
          diff: '@@ -1,5 +1,6 @@\n defmodule Acme.Docs do\n   @moduledoc """\n-  The old words.\n+  The new words,\n+  over two lines.\n   """\n end',
        },
        { path: "NOTES.unknownext", status: "modified", diff: "@@ -1,1 +1,1 @@\n-def old\n+def new" },
      ],
      proposals: [],
    },
  });
  const plugin = await mountPlugin(page, dir, { review });
  const f = plugin.frame;
  const docs = f.locator('[data-filesec="lib/acme/docs.ex"] .diff-row');
  for (const text of ["The old words.", "The new words,", "over two lines."])
    await expect(docs.filter({ hasText: text }).locator(".hljs-string")).toHaveCount(1);
  const notes = f.locator('[data-filesec="NOTES.unknownext"] .diff-row .code');
  await expect(notes.first()).toContainText("def");
  await expect(notes.locator("[class^='hljs-']")).toHaveCount(0);
});

test("in an app that serves no highlighter, the code stays plain and the view works", async ({ page }) => {
  // as an older app: the highlighting script leaves nothing behind
  await page.addInitScript(() => {
    Object.defineProperty(window, Symbol.for("pinrail.highlight"), { get: () => undefined, set: () => {} });
  });
  const plugin = await mountPlugin(page, dir, { review: round2() });
  const f = plugin.frame;
  const tickets = f.locator('[data-filesec="lib/acme/tickets.ex"]');
  await expect(tickets.locator(".diff-row").filter({ hasText: "def do_save(items) do" })).toHaveCount(1);
  await expect(tickets.locator("[class^='hljs-']")).toHaveCount(0);
  await expect(f.locator("#card-18")).toContainText("|> Enum.uniq_by(& &1.id)");
});

const webhooks = () => fixture(path.join(dir, "fixtures", "webhooks.json"));

test("in a narrow frame the file tree opens as a drawer over the diff, and a finding in it goes there", async ({
  page,
}) => {
  await page.setViewportSize({ width: 420, height: 800 });
  const plugin = await mountPlugin(page, dir, { review: webhooks() });
  const f = plugin.frame;
  await expect(f.locator(".card").first()).toBeVisible();
  const rail = f.locator("aside.rail");
  await expect(rail).toHaveCount(0);
  // the diff fills the width of the frame
  expect((await f.locator("#main").boundingBox())!.width).toBeGreaterThan(400);

  await f.getByRole("button", { name: "Show the file tree" }).click();
  await expect(rail).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(rail).toHaveCount(0);
  await expect(f.getByRole("button", { name: "Show the file tree" })).toBeFocused();

  await f.getByRole("button", { name: "Show the file tree" }).click();
  await rail.locator(".tree-finding", { hasText: "Add jitter" }).click();
  await expect(rail).toHaveCount(0);
  await expect(f.locator(".card", { hasText: "Add jitter" })).toBeInViewport();
  // the drawer belongs to the narrow layout and leaves the tree setting unchanged
  expect(await plugin.lastSettingsSet()).toBeNull();
});

test("in a narrow frame the title keeps a line, and a card's buttons go under its title", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 800 });
  const plugin = await mountPlugin(page, dir, { review: webhooks() });
  const f = plugin.frame;
  const title = (await f.locator(".head-title h1").boundingBox())!;
  expect(title.width).toBeGreaterThan(300);
  const card = f.locator(".card").first();
  const name = (await card.locator(".card-title").boundingBox())!;
  const accept = (await card.getByRole("button", { name: "Accept" }).boundingBox())!;
  expect(accept.y).toBeGreaterThanOrEqual(name.y + name.height);
  // the toolbar's buttons keep their accessible names
  await expect(f.getByRole("button", { name: "Fold all" })).toBeVisible();
});

test("the toolbar's arrows go through the undecided proposals, as n and p do, and say where the focus is", async ({
  page,
}) => {
  const plugin = await mountPlugin(page, dir, { review: webhooks() });
  const f = plugin.frame;
  const bar = f.locator('[role="toolbar"]');
  const count = bar.locator(".step-count");
  // the first proposal has the focus when the review opens
  await expect(count).toHaveText("1 of 5");

  const next = bar.getByRole("button", { name: "Next undecided proposal" });
  await next.click();
  await expect(count).toHaveText("2 of 5");
  await expect(f.locator(".card.focused")).toBeInViewport();
  // the button keeps the focus, so Enter moves to the next proposal
  await expect(next).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(count).toHaveText("3 of 5");

  // a decided proposal is skipped in both directions
  await f.locator(".card.focused").getByRole("button", { name: "Accept" }).click();
  await bar.getByRole("button", { name: "Previous undecided proposal" }).click();
  await expect(count).toHaveText("2 of 5");
  await bar.getByRole("button", { name: "Next undecided proposal" }).click();
  await expect(count).toHaveText("4 of 5");
});

test("a proposal's note and your own comment can be dictated, from a microphone beside their buttons", async ({
  page,
}) => {
  const plugin = await mountPlugin(page, dir, { review: webhooks(), dictation: true });
  const f = plugin.frame;
  const card = f.locator(".card").first();
  await card.getByRole("button", { name: "Add instruction" }).click();
  const note = card.locator("textarea[data-note-ta]");
  await expect(note).toBeFocused();
  await card.locator("[data-pinrail-dictate-for]").getByRole("button", { name: "Dictate" }).click();
  await plugin.say("re-enqueue it");
  await plugin.say("re-enqueue it with a run at");
  await expect(note).toHaveValue("re-enqueue it with a run at");
  await card.getByRole("button", { name: "Stop dictating" }).click();
  await expect.poll(() => plugin.dictating()).toBe(false);
});

test("without the app's dictation a note has no microphone", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: webhooks() });
  const card = plugin.frame.locator(".card").first();
  await card.getByRole("button", { name: "Add instruction" }).click();
  await expect(card.locator("textarea[data-note-ta]")).toBeVisible();
  await expect(plugin.frame.getByRole("button", { name: "Dictate" })).toHaveCount(0);
});

test("a diff as git diff prints it shows no header lines and counts only the changed lines", async ({ page }) => {
  const review = round2();
  const file = review.payload.files[0];
  file.diff = [
    "diff --git a/lib/acme/tickets.ex b/lib/acme/tickets.ex",
    "index 3b18e51..a9c2f04 100644",
    "--- a/lib/acme/tickets.ex",
    "+++ b/lib/acme/tickets.ex",
    file.diff,
  ].join("\n");
  const plugin = await mountPlugin(page, dir, { review });
  const section = plugin.frame.locator('[data-filesec="lib/acme/tickets.ex"]');
  await expect(section).toContainText("+3 −2");
  for (const header of ["diff --git", "index 3b18e51", "a/lib/acme/tickets.ex"]) {
    await expect(section.locator(".diff-row").filter({ hasText: header })).toHaveCount(0);
  }
});
