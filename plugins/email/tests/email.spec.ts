import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "../../../wicket_sdk/testing/playwright";

const dir = path.resolve(__dirname, "..");
const renewals = () => fixture(path.join(dir, "fixtures", "renewals.json"));
const northwind = () => renewals().payload.drafts[0];

/* Leaving drafts undecided asks for a confirmation first, which these tests
   are not about; the confirmation has its own test below. */
async function handOverPastTheWarning(plugin: Awaited<ReturnType<typeof mountPlugin>>) {
  await plugin.collect();
  await expect(plugin.frame.locator("#confirm")).toBeVisible();
  await plugin.collect();
}

test("renders every draft with its addresses, subject and body", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: renewals() });
  const f = plugin.frame;

  await expect(f.locator("[data-draft]")).toHaveCount(3);
  const first = f.locator('[data-draft="northwind"]');
  await expect(first.locator(".addresses")).toContainText("priya@northwind.example");
  await expect(first.locator(".addresses")).toContainText("sam@acme.com");
  await expect(first.locator('input[data-act="subject"]')).toHaveValue("Your Acme renewal on 12 October");
  await expect(first.locator("[data-body]")).toContainText("Your team's usage is up 40%");
  await expect(first.locator(".why")).toContainText("did not offer a discount");

  // The thread it replies to is there but folded away.
  const second = f.locator('[data-draft="brightside"]');
  await expect(second.locator(".thread summary")).toContainText("2 messages");
  await expect(second.locator(".thread .msg").first()).not.toBeVisible();
  await second.locator(".thread summary").click();
  await expect(second.locator(".thread .msg").first()).toContainText("nightly export failed");
});

test("an edit shows against the agent's words and travels as a replacement", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: renewals() });
  const first = plugin.frame.locator('[data-draft="northwind"]');

  await first.locator('[data-act="edit"]').click();
  const area = first.locator("textarea");
  await area.fill(northwind().body.replace("at your earliest convenience", "this week"));
  await first.locator('[data-act="edit"]').click();

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

test("a subject change is an edit too, and revert puts the draft back", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: renewals() });
  const first = plugin.frame.locator('[data-draft="northwind"]');

  await first.locator('input[data-act="subject"]').fill("Renewal on 12 October");
  await expect(first.locator(".subject")).toHaveClass(/changed/);

  await first.locator('[data-act="edit"]').click();
  await first.locator("textarea").fill("Hi Priya,\n\nShort and to the point.\n\nSam");
  await first.locator('[data-act="edit"]').click();
  await expect(first.locator('[data-act="revert"]')).toBeVisible();

  await first.locator('[data-act="revert"]').click();
  await expect(first.locator("[data-body]")).toContainText("Your team's usage is up 40%");
  await expect(first.locator("del")).toHaveCount(0);

  await first.getByRole("button", { name: "Send" }).click();
  await handOverPastTheWarning(plugin);
  const sent = (await plugin.nextSubmit()).drafts[0];
  expect(sent.subject).toBe("Renewal on 12 October");
  expect(sent.edits).toEqual([{ from: "Your Acme renewal on 12 October", to: "Renewal on 12 October" }]);
});

test("highlighting a passage hangs an instruction on it, shown apart from the draft", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: renewals() });
  const f = plugin.frame;
  const first = f.locator('[data-draft="northwind"]');

  // Select "I wanted to reach out" in the body, the way a reader would.
  await first.locator("[data-body]").evaluate((body) => {
    const text = [...body.childNodes].find((n) => n.textContent!.includes("reach out"))!;
    const at = text.textContent!.indexOf("I wanted to reach out");
    const range = document.createRange();
    range.setStart(text, at);
    range.setEnd(text, at + "I wanted to reach out".length);
    const selection = document.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    document.dispatchEvent(new Event("selectionchange"));
  });

  await f.locator("#pick button").click();
  await expect(first.locator(".mark .quote")).toHaveText("“I wanted to reach out”");
  // The passage is marked in the body, so the instruction has a place on the page.
  await expect(first.locator(".quoted")).toHaveText("I wanted to reach out");

  await first.locator('input[data-act="mark-note"]').fill("we never say reach out");
  await first.getByRole("button", { name: "Revise" }).click();
  await handOverPastTheWarning(plugin);
  const decided = (await plugin.nextSubmit()).drafts[0];
  expect(decided.action).toBe("revise");
  expect(decided.comments).toEqual([{ quote: "I wanted to reach out", note: "we never say reach out" }]);
});

test("undecided drafts need a confirmation and are reported as undecided", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: renewals() });
  const f = plugin.frame;

  await f.locator('[data-draft="northwind"]').getByRole("button", { name: "Send" }).click();
  await f.locator('[data-draft="kestrel"]').getByRole("button", { name: "Discard" }).click();
  await f.locator('[data-draft="kestrel"] input[data-act="note"]').fill("finance should send this, not us");

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
  const plugin = await mountPlugin(page, dir, { gate: renewals() });
  const f = plugin.frame;
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over");

  await f.locator('[data-draft="northwind"]').getByRole("button", { name: "Send" }).click();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over: send 1");

  await f.locator('[data-draft="brightside"]').getByRole("button", { name: "Revise" }).click();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over: send 1, revise 1");
});

test("edits, marks and verdicts survive a reload", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: renewals() });
  const first = plugin.frame.locator('[data-draft="northwind"]');

  await first.getByRole("button", { name: "Send" }).click();
  await first.locator('[data-act="edit"]').click();
  await first.locator("textarea").fill(northwind().body.replace("Best regards", "Thanks"));
  await first.locator('[data-act="edit"]').click();
  await expect.poll(() => plugin.lastDraft()).not.toBeNull();

  await plugin.reload();
  await plugin.reinit();
  const back = plugin.frame.locator('[data-draft="northwind"]');
  await expect(back.getByRole("button", { name: "Send" })).toHaveAttribute("aria-pressed", "true");
  await expect(back.locator("ins")).toHaveText("Thanks,");
  await expect(back.locator("del")).toHaveText("Best regards,");
});

test("a decided gate is read-only and shows what was sent and why", async ({ page }) => {
  const gate = {
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
  const plugin = await mountPlugin(page, dir, { gate, readonly: true });
  const f = plugin.frame;

  await expect(f.locator(".done")).toContainText("1 to send");
  await expect(f.locator(".done")).toContainText("2 left undecided");
  await expect(f.locator('[data-draft="northwind"] .verdict-ro')).toHaveText("send");
  await expect(f.locator('[data-draft="northwind"] .note-ro').first()).toHaveText("we never say reach out");
  await expect(f.locator("#pick")).toBeHidden();
  await expect(f.locator("button[data-act=verdict]")).toHaveCount(0);

  // A decided gate still shows what the human changed, against what was drafted.
  await expect(f.locator('[data-draft="northwind"] del')).toHaveText("at your earliest convenience,");
  await expect(f.locator('[data-draft="northwind"] ins')).toHaveText("this week,");
});
