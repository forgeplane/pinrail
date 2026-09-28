import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

const dir = path.resolve(__dirname, "..");
const renewals = () => fixture(path.join(dir, "fixtures", "renewals.json"));
const northwind = () => renewals().payload.drafts[0];

/* One draft is open at a time; the rail opens the others. */
async function open(plugin: Awaited<ReturnType<typeof mountPlugin>>, id: string) {
  await plugin.frame.locator(`[data-pick-id="${id}"]`).click();
  return plugin.frame.locator(`[data-draft="${id}"]`);
}

/* Selects a passage of the open draft's body the way a reader would. */
async function selectIn(draft: any, text: string) {
  await draft.locator("[data-body]").evaluate((body: HTMLElement, wanted: string) => {
    const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (node.parentElement!.closest("del")) continue;
      const at = node.textContent!.indexOf(wanted);
      if (at < 0) continue;
      body.focus();
      const range = document.createRange();
      range.setStart(node, at);
      range.setEnd(node, at + wanted.length);
      const selection = document.getSelection()!;
      selection.removeAllRanges();
      selection.addRange(range);
      document.dispatchEvent(new Event("selectionchange"));
      return;
    }
    throw new Error(`no "${wanted}" in the body`);
  }, text);
}

/* Types over a passage of the body, as an edit in place. */
async function rewrite(page: any, draft: any, from: string, to: string) {
  await selectIn(draft, from);
  await page.keyboard.type(to);
}

/* Leaving drafts undecided asks for a confirmation first, which these tests
   are not about; the confirmation has its own test below. */
async function handOverPastTheWarning(plugin: Awaited<ReturnType<typeof mountPlugin>>) {
  await plugin.collect();
  await expect(plugin.frame.locator("#confirm")).toBeVisible();
  await plugin.collect();
}

test("renders every draft with its addresses, subject and body", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const f = plugin.frame;

  await expect(f.locator("[data-pick]")).toHaveCount(3);
  const first = f.locator('[data-draft="northwind"]');
  await expect(first.locator(".envelope")).toContainText("priya@northwind.example");
  await expect(first.locator(".envelope")).toContainText("sam@acme.com");
  await expect(first.locator('input[data-act="subject"]')).toHaveValue("Your Acme renewal on 12 October");
  await expect(first.locator("[data-body]")).toContainText("Your team's usage is up 40%");
  await expect(first.locator(".why")).toContainText("did not offer a discount");

  // The thread it replies to is there but folded away.
  const second = await open(plugin, "brightside");
  await expect(second.locator(".thread summary")).toContainText("2 messages");
  await expect(second.locator(".thread .msg").first()).not.toBeVisible();
  await second.locator(".thread summary").click();
  await expect(second.locator(".thread .msg").first()).toContainText("nightly export failed");
});

test("an edit shows against the agent's words and travels as a replacement", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const first = plugin.frame.locator('[data-draft="northwind"]');

  await rewrite(page, first, "at your earliest convenience", "this week");

  // What the agent wrote is still on screen, struck through, beside what replaced it.
  await expect(first.locator("del")).toHaveText("at your earliest convenience,");
  await expect(first.locator("ins")).toHaveText("this week,");

  await first.getByRole("button", { name: "Send" }).click();
  await handOverPastTheWarning(plugin);
  const data = await plugin.nextSubmit();
  const sent = data.drafts.find((d: any) => d.id === "northwind");
  expect(sent.action).toBe("send");
  expect(sent.body).toContain("this week");
  expect(sent.body).not.toContain("at your earliest convenience");
  expect(sent.edits).toEqual([{ from: "at your earliest convenience,", to: "this week," }]);
});

test("the subject keeps what it was under it, and revert all puts the draft back", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const f = plugin.frame;
  const first = f.locator('[data-draft="northwind"]');
  const subject = first.locator('input[data-act="subject"]');

  await subject.fill("Renewal on 12 October");
  await expect(first.locator(".subject")).toHaveClass(/changed/);
  await expect(first.locator(".subject .was")).toHaveText("was Your Acme renewal on 12 October");
  await expect(f.locator(".aside .change")).toHaveCount(1);
  await expect(f.locator(".aside .change-kind")).toHaveText("subject");
  await expect(f.locator('[data-pick-id="northwind"] .pick-subject')).toHaveText("Renewal on 12 October");

  await rewrite(page, first, "Your team's usage is up 40%", "Short and to the point");
  await expect(f.locator(".aside .change")).toHaveCount(2);

  await f.locator('[data-act="revert"]').click();
  await expect(first.locator("[data-body]")).toContainText("Your team's usage is up 40%");
  await expect(first.locator(".body del")).toHaveCount(0);
  await expect(subject).toHaveValue("Your Acme renewal on 12 October");
  await expect(first.locator(".subject .was")).toHaveCount(0);
  await expect(f.locator(".aside .change")).toHaveCount(0);

  await subject.fill("Renewal on 12 October");
  await first.getByRole("button", { name: "Send" }).click();
  await handOverPastTheWarning(plugin);
  const sent = (await plugin.nextSubmit()).drafts[0];
  expect(sent.subject).toBe("Renewal on 12 October");
  expect(sent.edits).toEqual([{ from: "Your Acme renewal on 12 October", to: "Renewal on 12 October" }]);
});

test("every change is listed beside the draft and can be put back on its own", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const f = plugin.frame;
  const first = f.locator('[data-draft="northwind"]');
  const changes = f.locator(".aside .change");

  await first.locator('input[data-act="subject"]').fill("Renewal on 12 October");
  await rewrite(page, first, "at your earliest convenience", "this week");
  await rewrite(page, first, "Best regards", "Thanks");
  await expect(changes).toHaveCount(3);
  await expect(changes.nth(0)).toContainText("subject");
  await expect(changes.nth(1).locator("del")).toHaveText("at your earliest convenience,");
  await expect(changes.nth(1).locator("ins")).toHaveText("this week,");
  await expect(changes.nth(2).locator("ins")).toHaveText("Thanks,");

  // one goes back; the subject and the other edit stay
  await changes.nth(1).locator('[data-act="revert-edit"]').click();
  await expect(changes).toHaveCount(2);
  await expect(first.locator("[data-body]")).toContainText("at your earliest convenience");
  await expect(first.locator(".body del")).toHaveText("Best regards,");
  await expect(first.locator(".body ins")).toHaveText("Thanks,");
  await expect(first.locator('input[data-act="subject"]')).toHaveValue("Renewal on 12 October");

  // putting one back is itself a step undo takes back
  await first.locator("[data-body]").focus();
  await page.keyboard.press("ControlOrMeta+z");
  await expect(changes).toHaveCount(3);
  await expect(first.locator(".body ins")).toHaveText(["this week,", "Thanks,"]);
});

test("undo in the subject takes back the subject, then the body, in the order they changed", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const first = plugin.frame.locator('[data-draft="northwind"]');
  const subject = first.locator('input[data-act="subject"]');

  await rewrite(page, first, "Best regards", "Thanks");
  await subject.fill("Renewal on 12 October");
  await expect(first.locator(".subject .was")).toBeVisible();

  await page.keyboard.press("ControlOrMeta+z");
  await expect(subject).toHaveValue("Your Acme renewal on 12 October");
  await expect(first.locator(".subject .was")).toHaveCount(0);
  await expect(first.locator(".body ins")).toHaveText("Thanks,");

  for (let i = 0; i < 6; i++) await page.keyboard.press("ControlOrMeta+z");
  await expect(first.locator(".body ins")).toHaveCount(0);
  await expect(plugin.frame.locator(".aside .change")).toHaveCount(0);
});

test("highlighting a passage hangs an instruction on it, shown apart from the draft", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const f = plugin.frame;
  const first = f.locator('[data-draft="northwind"]');

  await selectIn(first, "I wanted to reach out");

  // a button beside the selection opens a popover for the instruction
  await f.locator("#pick button").click();
  await expect(f.locator("[data-popover]")).toContainText("I wanted to reach out");
  await f.locator("[data-mark-text]").fill("we never say reach out");
  await f.locator("[data-mark-save]").click();
  await expect(f.locator("[data-popover]")).toHaveCount(0);

  await expect(f.locator(".aside .mark .quote")).toHaveText("“I wanted to reach out”");
  await expect(f.locator('.aside input[data-act="mark-note"]')).toHaveValue("we never say reach out");
  // The passage is marked in the body, so the instruction has a place on the page.
  await expect(first.locator(".quoted")).toHaveText("I wanted to reach out");
  await first.getByRole("button", { name: "Revise" }).click();
  await handOverPastTheWarning(plugin);
  const decided = (await plugin.nextSubmit()).drafts[0];
  expect(decided.action).toBe("revise");
  expect(decided.comments).toEqual([{ quote: "I wanted to reach out", note: "we never say reach out" }]);
});

test("undecided drafts need a confirmation and are reported as undecided", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const f = plugin.frame;

  await f.locator('[data-draft="northwind"]').getByRole("button", { name: "Send" }).click();
  const kestrel = await open(plugin, "kestrel");
  await kestrel.getByRole("button", { name: "Discard" }).click();
  await kestrel.locator('[data-act="note"]').fill("finance should send this, not us");

  await plugin.collect();
  await expect(f.locator("#confirm")).toContainText("1 draft is still undecided");
  expect((await plugin.messages()).filter((m) => m.type === "submit")).toHaveLength(0);

  await plugin.collect();
  const data = await plugin.nextSubmit();
  expect(data.undecided).toEqual(["brightside"]);
  expect(data.drafts.map((d: any) => [d.id, d.action])).toEqual([["northwind", "send"], ["kestrel", "discard"]]);
  expect(data.drafts.find((d: any) => d.id === "kestrel").note).toBe("finance should send this, not us");
});

test("the hand-over label says what it would do", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const f = plugin.frame;
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over");

  await f.locator('[data-draft="northwind"]').getByRole("button", { name: "Send" }).click();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over: send 1");

  await (await open(plugin, "brightside")).getByRole("button", { name: "Revise" }).click();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over: send 1, revise 1");
});

test("edits, marks and verdicts survive a reload", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const first = plugin.frame.locator('[data-draft="northwind"]');

  await first.getByRole("button", { name: "Send" }).click();
  await rewrite(page, first, "Best regards", "Thanks");
  await expect.poll(async () => JSON.stringify(await plugin.lastDraft())).toContain("Thanks");

  await plugin.reload();
  await plugin.reinit();
  const back = plugin.frame.locator('[data-draft="northwind"]');
  await expect(back.getByRole("button", { name: "Send" })).toHaveAttribute("aria-pressed", "true");
  await expect(back.locator(".body ins")).toHaveText("Thanks,");
  await expect(back.locator(".body del")).toHaveText("Best regards,");
});

test("a decided review is read-only and shows what was sent and why", async ({ page }) => {
  const review = {
    ...renewals(),
    status: "decided",
    decision: {
      decided_by: "sam",
      data: {
        drafts: [
          { id: "northwind", action: "send", subject: "Your Acme renewal on 12 October",
            body: northwind().body.replace("at your earliest convenience", "this week"),
            comments: [{ quote: "reach out", note: "we never say reach out" }], note: "fine otherwise" },
        ],
        undecided: ["brightside", "kestrel"],
      },
    },
  };
  const plugin = await mountPlugin(page, dir, { review, readonly: true });
  const f = plugin.frame;

  await expect(f.locator(".done")).toContainText("1 to send");
  await expect(f.locator(".done")).toContainText("2 left undecided");
  await expect(f.locator('[data-draft="northwind"] .verdict-ro')).toHaveText("send");
  await expect(f.locator(".aside .note-ro")).toHaveText("we never say reach out");
  await expect(f.locator(".aside .change")).toHaveCount(1);
  await expect(f.locator('[data-act="revert-edit"], [data-act="revert"]')).toHaveCount(0);
  await expect(f.locator("#pick")).toBeHidden();
  await expect(f.locator("button[data-act=verdict]")).toHaveCount(0);

  // A decided review still shows what the human changed, against what was drafted.
  await expect(f.locator('[data-draft="northwind"] del')).toHaveText("at your earliest convenience,");
  await expect(f.locator('[data-draft="northwind"] ins')).toHaveText("this week,");
});

test("the rail opens each draft, the keys move and decide, and the verdict shows on the rail", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const f = plugin.frame;
  await expect(f.locator('[data-pick-id="northwind"]')).toHaveAttribute("aria-current", "true");

  await f.locator("h1").click();
  await f.locator("body").press("s");
  await expect(f.locator('[data-pick-id="northwind"] .chip')).toHaveText("Send");
  await f.locator("body").press("j");
  await expect(f.locator('[data-draft="brightside"]')).toBeVisible();
  await f.locator("body").press("x");
  await expect(f.locator('[data-pick-id="brightside"] .chip')).toHaveText("Discard");
  await f.locator("body").press("k");
  await expect(f.locator('[data-draft="northwind"]')).toBeVisible();

  // e puts the cursor in the message; typing a j there types it
  await f.locator("body").press("e");
  const body = f.locator('[data-draft="northwind"] [data-body]');
  await expect(body).toBeFocused();
  await page.keyboard.type(" j");
  await expect(body.locator("ins")).toHaveText("j");
  await expect(f.locator('[data-draft="northwind"]')).toBeVisible();

  // shift+s sends every draft still undecided
  await f.locator("h1").click();
  await f.locator("body").press("Shift+S");
  await expect(f.locator('[data-pick-id="kestrel"] .chip')).toHaveText("Send");
  await expect(f.locator('[data-pick-id="brightside"] .chip')).toHaveText("Discard");
});

test("editing in place tracks every change, and undo takes them back", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const first = plugin.frame.locator('[data-draft="northwind"]');

  // type over a phrase, then delete a word with backspace
  await rewrite(page, first, "at your earliest convenience", "this week");
  await expect(first.locator(".body del")).toHaveText("at your earliest convenience,");
  await expect(first.locator(".body ins")).toHaveText("this week,");
  // the struck-through words are not text: the cursor stays in the working copy
  for (let i = 0; i < 5; i++) await page.keyboard.press("Backspace");
  await expect(first.locator(".body ins")).toHaveText("this,");
  await page.keyboard.type(" Friday");
  await expect(first.locator(".body ins")).toHaveText("this Friday,");

  // a new line is a new line
  await page.keyboard.press("Enter");
  await expect.poll(() => first.locator("[data-body]").evaluate((b) => b.textContent)).toContain("this Friday\n,");

  // undo walks back through the edits, one at a time
  for (let i = 0; i < 30; i++) await page.keyboard.press("ControlOrMeta+z");
  await expect(first.locator(".body del")).toHaveCount(0);
  await expect(first.locator(".body ins")).toHaveCount(0);
});

test("redo puts an undone edit back", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const first = plugin.frame.locator('[data-draft="northwind"]');
  await rewrite(page, first, "Best regards", "Thanks");
  for (let i = 0; i < 6; i++) await page.keyboard.press("ControlOrMeta+z");
  await expect(first.locator(".body ins")).toHaveCount(0);
  for (let i = 0; i < 6; i++) await page.keyboard.press("ControlOrMeta+Shift+z");
  await expect(first.locator(".body ins")).toHaveText("Thanks,");
});

test("a sentence rewritten is one change, and puts back as one", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const f = plugin.frame;
  const first = f.locator('[data-draft="northwind"]');
  // a few words of the old sentence survive between the new ones
  await rewrite(page, first, "at your earliest convenience", "this week, at your desk if you can");
  const changes = f.locator(".aside .change");
  await expect(changes).toHaveCount(1);
  await changes.first().locator('[data-act="revert-edit"]').click();
  await expect(changes).toHaveCount(0);
  await expect(first.locator("[data-body]")).toContainText("at your earliest convenience");
  await expect(first.locator(".body ins")).toHaveCount(0);
});

test("a refused hand-over says why and keeps every edit", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: renewals() });
  const first = plugin.frame.locator('[data-draft="northwind"]');
  await rewrite(page, first, "at your earliest convenience", "this week");
  await first.getByRole("button", { name: "Send" }).click();
  await handOverPastTheWarning(plugin);
  await plugin.nextSubmit();

  await plugin.sendViolations([{ path: "/drafts/0/body", message: "must not be empty" }]);
  const view = plugin.frame.locator("body");
  await expect(view).toContainText("/drafts/0/body: must not be empty");
  // the draft, marked to send, still carries its edit: collapsed, "edited"
  await expect(view).toContainText("edited");
  await expect(view).toContainText("1 to send");
});
