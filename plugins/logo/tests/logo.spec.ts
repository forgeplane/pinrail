import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "wicket-plugin/testing";

// The view alone, under the harness: no app, no CLI.
const dir = path.resolve(__dirname, "..");
const round = () => fixture(path.join(dir, "fixtures", "tidemark.json"));

test("shows every mark in the rail, and the chosen one in every place it will live", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round() });
  const f = plugin.frame;
  await expect(f.locator(".pick")).toHaveCount(6);
  await expect(f.locator("#mark-name")).toHaveText("Three waves");
  // large on both palettes, a ladder of sizes, tabs, icons, menu bars, lockups
  await expect(f.locator(".stage")).toHaveCount(2);
  await expect(f.locator(".ladder-row .rung")).toHaveCount(14);
  await expect(f.locator(".tab.active")).toHaveCount(2);
  await expect(f.locator(".menubar")).toHaveCount(2);
  // the wordmark's [brackets] are set in the accent
  await expect(f.locator(".lockup .wordmark .acc").first()).toHaveText("mark");
  // the reasoning renders as markdown
  await expect(f.locator(".reasoning")).toContainText("16 px");
});

test("an agent's svg is drawn without scripts, handlers or outside links", async ({ page }) => {
  const gate = round();
  gate.payload.marks = [{
    id: "evil", name: "Evil", reasoning: "",
    svg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" onload="window.pwned=1"><script>window.pwned=2</script><style>body{display:none}</style><a href="https://example.com"><rect width="4" height="4"/></a><rect width="24" height="24" fill="url(https://example.com/x)"/><circle cx="12" cy="12" r="6" onclick="window.pwned=3"/></svg>',
  }];
  const plugin = await mountPlugin(page, dir, { gate });
  const f = plugin.frame;
  await expect(f.locator(".stage svg").first()).toBeVisible();
  const html = await f.locator(".stage").first().innerHTML();
  expect(html).not.toMatch(/script|onload|onclick|<style|example\.com|<a /);
  expect(await f.locator("body").evaluate(() => (window as any).pwned)).toBeUndefined();
});

test("one favourite, keys to decide, and a warning before undecided marks go back", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round() });
  const f = plugin.frame;
  await f.locator("body").click({ position: { x: 600, y: 5 } });

  await f.locator("body").press("f");                               // M1 favourite
  await f.locator("body").press("j");
  await f.locator("body").press("x");                               // M2 dropped
  await f.locator("#note").fill("Too much like a ring");
  await f.locator("body").click({ position: { x: 600, y: 5 } });
  await f.locator("body").press("j");
  await f.locator("body").press("j");
  await f.locator('.choice[data-action="favorite"]').click();  // M4 favourite: M1 steps down
  await expect(f.locator(".pick").nth(0).locator(".verdict-chip")).toHaveText("Keep");
  await expect(f.locator(".pick").nth(3).locator(".verdict-chip")).toHaveText("★ Favourite");

  // three are undecided: the first hand-over only asks
  await plugin.collect();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over with 3 undecided");
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({
    decisions: [{ id: "M1", action: "keep" }, { id: "M2", action: "drop", note: "Too much like a ring" }, { id: "M4", action: "favorite" }],
    undecided: ["M3", "M5", "M6"],
  });
});

test("a part of the mark can be picked and commented on, and the mark counts as kept", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round() });
  const f = plugin.frame;
  await f.locator(".pick").nth(3).click();                               // M4: two posts and the line
  // the second rect is the right post; click it on the light stage
  await f.locator('.stage[data-stage="light"] svg rect').nth(1).click({ force: true });
  await expect(f.locator("#composer")).toContainText("rect 2");
  await f.locator("#part-note").fill("A little thinner");
  await f.locator("#part-note").press("Enter");
  await expect(f.locator(".part-list li")).toHaveCount(1);
  await expect(f.locator(".stage .part-pin")).toHaveCount(2);            // pinned on both palettes
  await expect(f.locator(".pick").nth(3).locator(".verdict-chip")).toHaveText("Keep");

  await plugin.collect();
  await plugin.collect();
  const decision = await plugin.nextSubmit();
  expect(decision.decisions).toEqual([{
    id: "M4", action: "keep",
    comments: [{ target: "svg > rect:nth-of-type(2)", tag: "rect", markup: expect.stringContaining("<rect"), note: "A little thinner" }],
  }]);
});

test("a decided round is read-only and shows what was decided", async ({ page }) => {
  const gate = fixture(path.join(dir, "fixtures", "tidemark.decided.json"));
  const plugin = await mountPlugin(page, dir, { gate, readonly: true });
  const f = plugin.frame;
  await expect(f.locator(".pick").nth(3).locator(".verdict-chip")).toHaveText("★ Favourite");
  await expect(f.locator('.choice[data-action="keep"]')).toBeDisabled();
  await f.locator(".pick").nth(3).click();
  await expect(f.locator("#note")).toHaveValue("Pixel-tune it at 16");
});
