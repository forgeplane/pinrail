import { expect as base, test } from "@playwright/test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fixture, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

// The view alone, under the harness: no app, no CLI. WebGL runs in software.
const dir = path.resolve(__dirname, "..");
const round = () => fixture(path.join(dir, "fixtures", "halden.json"));
test.use({ launchOptions: { args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] } });
// Every model is read and its still drawn in software; on a CI runner the
// last of them can take longer than the default five seconds.
const expect = base.configure({ timeout: 20_000 });

test("shows every model in the rail, and the chosen one on the stage with its views and size", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round() });
  const f = plugin.frame;
  await expect(f.locator(".pick")).toHaveCount(4);
  await expect(f.locator(".pick .still img")).toHaveCount(4);
  await expect(f.locator("#model-name")).toHaveText("Pivot");
  await expect(f.locator("#viewer canvas")).toBeVisible();
  // the set views, then the agent's own
  await expect(f.locator("[data-view]")).toHaveText(["¾ 1", "Front 2", "Side 3", "Top 4", "Seated 5"]);
  await expect(f.locator(".stats")).toContainText("cm");
  await expect(f.locator(".stats")).toContainText("8 parts");
  // the parts by the names the agent gave them, spaces and all
  await expect(f.locator("[data-part]").filter({ hasText: "Lower tube" })).toHaveCount(1);
  await expect(f.locator(".reasoning")).toContainText("joints and the switch");
});

test("one favourite, keys to decide, and a warning before undecided models go back", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round() });
  const f = plugin.frame;
  await expect(f.locator("#viewer canvas")).toBeVisible();
  await f.locator("body").click({ position: { x: 600, y: 5 } });

  await f.locator("body").press("f");                               // L1 favourite
  await f.locator("body").press("j");
  await f.locator("body").press("x");                               // L2 dropped
  await f.locator("#note").fill("Cannot be aimed");
  await f.locator("body").click({ position: { x: 600, y: 5 } });
  await f.locator("body").press("j");
  await f.locator('.choice[data-action="favorite"]').click();      // L3 favourite: L1 steps down
  await expect(f.locator(".pick").nth(0).locator(".verdict-chip")).toHaveText("Keep");
  await expect(f.locator(".pick").nth(2).locator(".verdict-chip")).toHaveText("★ Favourite");

  await plugin.collect();
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over with 1 undecided");
  await plugin.collect();
  expect(await plugin.nextSubmit()).toEqual({
    decisions: [{ id: "L1", action: "keep" }, { id: "L2", action: "drop", note: "Cannot be aimed" }, { id: "L3", action: "favorite" }],
    undecided: ["L4"],
  });
});

test("a part picked in the list is commented on, pinned, and the model counts as kept", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round() });
  const f = plugin.frame;
  await f.locator("[data-part]").filter({ hasText: "Shade" }).click();
  await expect(f.locator("#pop")).toContainText("Lamp > Lower arm > Upper arm > Head > Shade");
  await f.locator("#part-note").fill("Wider and shallower");
  await f.locator("#part-note").press("Enter");
  await expect(f.locator(".comments li")).toHaveCount(1);
  await expect(f.locator(".pins .pin")).toHaveCount(1);
  await expect(f.locator(".pick").nth(0).locator(".verdict-chip")).toHaveText("Keep");

  await plugin.collect();
  await plugin.collect();
  const decision = await plugin.nextSubmit();
  expect(decision.decisions).toEqual([{
    id: "L1", action: "keep",
    comments: [{
      target: "Lamp > Lower arm > Upper arm > Head > Shade", name: "Shade", material: "Powder coat",
      point: [expect.any(Number), expect.any(Number), expect.any(Number)],
      view: { position: expect.any(Array), target: expect.any(Array) },
      note: "Wider and shallower",
    }],
  }]);
});

test("a click on the model itself opens a comment on the part under it", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round() });
  const f = plugin.frame;
  // the Column, front on: its drum shade fills the upper middle of the stage
  await f.locator(".pick").nth(2).click();
  await f.locator('[data-view="1"]').click();
  const box = (await f.locator("#viewer canvas").boundingBox())!;
  await page.mouse.click(box.x + box.width / 2, box.y + box.height * 0.3);
  await expect(f.locator("#pop")).toContainText("Drum shade");
  await page.keyboard.press("Escape");
  await expect(f.locator("#pop")).toHaveCount(0);
});

test("three.js JSON is read as well as GLB", async ({ page }) => {
  const gate = round();
  gate.payload = JSON.parse(fs.readFileSync(path.join(dir, "fixtures/inline-object.json"), "utf8"));
  const plugin = await mountPlugin(page, dir, { gate });
  const f = plugin.frame;
  await expect(f.locator("#model-name")).toHaveText("Column");
  await expect(f.locator("[data-part]")).toHaveText([/Base/, /Stem/, /Drum shade/]);
});

test("a model that cannot be read says so, and the others still show", async ({ page }) => {
  const gate = round();
  gate.payload.models[0].file = { $attachment: "broken.glb" };
  const broken = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "model-")), "broken.glb");
  fs.writeFileSync(broken, "not a model");
  const files = Object.fromEntries(["arc", "column", "tripod"].map((n) => [`${n}.glb`, `fixtures/halden/${n}.glb`]));
  const plugin = await mountPlugin(page, dir, { gate, attachments: { ...files, "broken.glb": broken } });
  const f = plugin.frame;
  await expect(f.locator(".broken")).toContainText("could not be read");
  await expect(f.locator(".pick .still img")).toHaveCount(3);
});

test("the models come from files the shell hands over, not from the payload", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { gate: round() });
  await expect(plugin.frame.locator(".pick .still img")).toHaveCount(4);
  const asked = (await plugin.messages()).filter((m: any) => m.type === "attachment").map((m: any) => m.name).sort();
  expect(asked).toEqual(["arc.glb", "column.glb", "pivot.glb", "tripod.glb"]);
});

test("a decided round is read-only and shows what was decided", async ({ page }) => {
  const gate = fixture(path.join(dir, "fixtures", "halden.decided.json"));
  const plugin = await mountPlugin(page, dir, { gate, readonly: true });
  const f = plugin.frame;
  await expect(f.locator(".pick").nth(0).locator(".verdict-chip")).toHaveText("★ Favourite");
  await expect(f.locator('.choice[data-action="keep"]')).toBeDisabled();
  await expect(f.locator(".comments li")).toHaveCount(2);
  await expect(f.locator("#note")).toHaveValue(/Warmer overall/);
});
