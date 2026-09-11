import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "../../../../../wicket_sdk/testing/playwright";

const dir = path.resolve(__dirname, "..");
const triage = () => fixture(path.join(dir, "fixtures", "triage.json"));
const round1 = () => fixture(path.join(dir, "fixtures", "triage-round-1.decided.json"));

test("renders groups, items, markdown and meta chips", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: triage() });
  const f = plugin.frame;
  await expect(f.locator(".intro")).toContainText("Sentry triage for acme-api");
  await expect(f.locator(".intro b")).toHaveText(["acme-api", "acme-worker"]);
  await expect(f.locator("h2.group")).toHaveText(["acme-api2", "acme-worker2"]);
  await expect(f.locator('[data-id="101"] .sev')).toHaveText("blocker");
  await expect(f.locator('[data-id="101"] .meta')).toHaveText(["issue: ACME-API-9F2", "count: 312"]);
  await expect(f.locator('[data-id="104"] pre')).toContainText("timeout after 60000ms");
  await expect(f.locator(".footer")).toContainText("0 accepted · 0 rejected · 4 undecided");
});

test("accept, reject with a note, an addition; the decision is exactly that", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: triage() });
  const f = plugin.frame;
  await f.locator('[data-id="101"] button', { hasText: "Accept" }).click();
  await f.locator('[data-id="102"] button', { hasText: "Reject" }).click();
  await f.getByLabel("note for item 102").fill("deploys are fine, fix the rollout instead");
  await f.getByRole("button", { name: "accept all undecided" }).click();
  await f.getByRole("button", { name: "+ add a note of your own" }).click();
  await f.getByLabel("addition 1", { exact: true }).fill("also check the importer");
  await f.getByLabel("group for addition 1").selectOption("acme-api");
  await expect(f.locator(".footer")).toContainText("3 accepted · 1 rejected · 0 undecided");
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({
    decisions: [
      { id: 101, action: "accept" },
      { id: 102, action: "reject", note: "deploys are fine, fix the rollout instead" },
      { id: 104, action: "accept" },
      { id: 105, action: "accept" },
    ],
    undecided: [],
    additions: [{ group: "acme-api", body: "also check the importer" }],
  });
});

test("undecided items need a confirmation and are reported as undecided", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: triage() });
  const f = plugin.frame;
  await f.locator('[data-id="101"] button', { hasText: "Accept" }).click();
  await plugin.collect();
  await expect(f.locator(".footer")).toContainText("3 left undecided");
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over anyway");
  expect((await plugin.messages()).filter((m) => m.type === "submit")).toHaveLength(0);

  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({ decisions: [{ id: 101, action: "accept" }], undecided: [102, 104, 105] });
});

test("the header and the group heading stay while the body scrolls under them", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: triage() });
  await expect(plugin.frame.locator(".item").first()).toBeVisible();
  await plugin.setFrameHeight(360);

  const pinned = await plugin.frame.locator(".plugin-scroll").evaluate((scroll) => {
    scroll.scrollTop = 300;
    const doc = scroll.ownerDocument;
    const header = doc.querySelector(".plugin-header").getBoundingClientRect();
    const subhead = doc.querySelector(".plugin-subhead").getBoundingClientRect();
    return {
      scrolled: scroll.scrollTop > 0,
      documentScrolled: doc.documentElement.scrollTop,
      headerTop: Math.round(header.top),
      headerBottom: Math.round(header.bottom),
      subheadTop: Math.round(subhead.top),
    };
  });

  expect(pinned.scrolled).toBe(true);
  expect(pinned.documentScrolled).toBe(0, "the body scrolls, not the document");
  expect(pinned.headerTop).toBe(0);
  expect(pinned.subheadTop).toBe(pinned.headerBottom, "the group pins under the header");
});

test("keeping deciding takes the warning back", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: triage() });
  await plugin.collect();
  await expect(plugin.frame.locator(".footer")).toContainText("4 left undecided");
  await plugin.frame.getByRole("button", { name: "keep deciding" }).click();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over");

  await plugin.frame.getByRole("button", { name: "accept all undecided" }).click();
  await plugin.collect();
  expect((await plugin.nextSubmit()).undecided).toEqual([]);
});

test("a click drafts at once, typing is debounced, and a reload restores both", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: triage() });
  const f = plugin.frame;
  await f.locator('[data-id="101"] button', { hasText: "Accept" }).click();
  await expect.poll(() => plugin.lastDraft().then((d) => d && d.decisions["101"] && d.decisions["101"].action)).toBe("accept");
  await f.getByLabel("note for item 101").fill("mention the importer");
  await expect.poll(() => plugin.lastDraft().then((d) => d.decisions["101"].note)).toBe("mention the importer");

  await plugin.reload();
  await plugin.reinit();
  await expect(f.locator('[data-id="101"].accepted')).toBeVisible();
  await expect(f.getByLabel("note for item 101")).toHaveValue("mention the importer");
});

test("violations show in the frame; submitted flips to read-only with verdicts", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: triage() });
  const f = plugin.frame;
  await plugin.sendViolations([{ path: "/decisions/0/action", message: "value must be one of the enum values" }]);
  await expect(f.locator("#errors")).toContainText("/decisions/0/action: value must be one of");

  await plugin.sendSubmitted({ decided_by: "alice", decided_at: "2026-09-11T10:00:00Z", data: { decisions: [{ id: 101, action: "reject", note: "nope" }], undecided: [102, 104, 105] } });
  await expect(f.locator(".done")).toContainText("Decided by alice: 0 accepted, 1 rejected, 3 undecided.");
  await expect(f.locator('[data-id="101"] .verdict')).toHaveText("reject");
  await expect(f.locator('[data-id="101"] .note-ro')).toContainText("nope");
  await expect(f.locator("button")).toHaveCount(0);
});

test("a superseding gate shows the previous round's verdicts; a withdrawn one reads as closed", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: triage(), previous: round1() });
  const f = plugin.frame;
  await expect(f.locator('[data-id="101"] .previous')).toHaveText("previous round: reject: not ours, it is the importer");
  await expect(f.locator('[data-id="102"] .previous')).toHaveText("previous round: undecided");
  await expect(f.locator('[data-id="104"] .previous')).toHaveCount(0);

  const withdrawn = await mountPlugin(page, dir, { gate: { ...triage(), status: "withdrawn" }, readonly: true });
  await expect(withdrawn.frame.locator(".done")).toHaveText("Closed without a decision (withdrawn). Read-only.");
});
