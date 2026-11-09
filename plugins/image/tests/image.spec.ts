import { expect, test } from "@playwright/test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
import { fixture, mountPlugin } from "pinrail-sdk/testing";

// The view alone, under the harness: no app, no CLI.
const dir = path.resolve(__dirname, "..");
const round = () => fixture(path.join(dir, "fixtures", "tern.json"));
const files = (names: string[]) => Object.fromEntries(names.map((n) => [n, `fixtures/tern/${n}`]));

async function ready(plugin: Awaited<ReturnType<typeof mountPlugin>>) {
  await expect(plugin.frame.locator(".pick .thumb img")).toHaveCount(4);
  await expect(plugin.frame.locator(".stage .art")).toBeVisible();
}
/** A spot on the stage's image, as fractions of it, in page coordinates. */
async function onImage(plugin: Awaited<ReturnType<typeof mountPlugin>>, x: number, y: number) {
  const box = (await plugin.frame.locator(".stage .art").boundingBox())!;
  return [box.x + box.width * x, box.y + box.height * y] as const;
}
const near = (n: number) => expect.closeTo(n, 1);

// Every decision a test takes is also held to the schema the app holds it to;
// ajv comes with the plugin toolkit.
const Ajv2020 = createRequire(require.resolve("pinrail-sdk/testing"))("ajv/dist/2020");
const schema = JSON.parse(fs.readFileSync(path.join(dir, "schemas", "decision.schema.json"), "utf8"));
const validate = new (Ajv2020.default ?? Ajv2020)({ allErrors: true, strict: false }).compile(schema);
function valid(decision: any): any {
  expect(validate(decision) ? [] : validate.errors).toEqual([]);
  return decision;
}

test("shows every image in the rail, whatever its format, and the chosen one on the stage", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  await ready(plugin);
  await expect(f.locator(".pick-meta")).toHaveText([
    "960×720 · PNG",
    "960×720 · JPEG",
    "960×720 · WebP",
    "960×720 · SVG",
  ]);
  await expect(f.locator("#image-name")).toHaveText("Paper plane");
  await expect(f.locator(".reasoning")).toContainText("flies off and is done");
  await expect(f.locator(".foot")).toContainText("960 × 720 px · PNG");
  await expect(f.locator(".facts")).toContainText("seed 48211");
  await expect(f.locator(".stage .art")).toHaveAttribute("alt", /paper plane loops/);
  // the files come from the shell, not from the payload
  const asked = (await plugin.messages())
    .filter((m: any) => m.type === "attachment")
    .map((m: any) => m.name)
    .sort();
  expect(asked).toEqual(["hammock.webp", "mailbox.jpg", "paper-plane.png", "tern.svg"]);
});

test("one favourite, keys to decide, and a warning before undecided images go back", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  await ready(plugin);
  await f.locator("body").click({ position: { x: 600, y: 5 } });
  await f.locator("body").press("f"); // A favourite
  await f.locator("body").press("n");
  await f.locator("body").press("x"); // B dropped
  await expect(f.locator('.choice[data-action="drop"]')).not.toHaveCSS("text-decoration-line", "line-through");
  await expect(f.locator("#note")).toHaveCount(0); // a note on request
  await f.getByRole("button", { name: "Add a note" }).click();
  await expect(f.locator("#note")).toBeFocused();
  await f.locator("#note").fill("The post floats");
  await f.locator("body").click({ position: { x: 600, y: 5 } });
  await f.locator("body").press("n");
  await f.locator('.choice[data-action="favorite"]').click(); // C favourite: A steps down
  await expect(f.locator(".pick").nth(0).locator(".verdict-chip")).toHaveText("Keep");
  await expect(f.locator(".pick").nth(2).locator(".verdict-chip")).toHaveText("★ Favourite");

  // the first hand-over asks, in the confirmation bar; Keep deciding takes it back
  await plugin.collect();
  const bar = f.locator(".pinrail-confirmation");
  await expect(bar).toHaveClass(/pinrail-confirmation-warning/);
  await expect(bar).toContainText("1 image left undecided");
  await expect.poll(() => plugin.lastStatus()).toBe("Hand over with 1 undecided");
  await bar.getByRole("button", { name: "Keep deciding" }).click();
  await expect(bar).toBeHidden();
  await plugin.collect();
  await expect(bar).toBeVisible();
  await plugin.collect();
  expect(valid(await plugin.nextSubmit())).toEqual({
    decisions: [
      { id: "A", action: "keep" },
      { id: "B", action: "drop", note: "The post floats" },
      { id: "C", action: "favorite" },
    ],
    undecided: ["D"],
  });
});

test("a single image fills the view, with no list of images beside it", async ({ page }) => {
  const review = fixture(path.join(dir, "fixtures", "single.json"));
  const plugin = await mountPlugin(page, dir, { review });
  const f = plugin.frame;
  await expect(f.locator(".stage .art")).toBeVisible();
  await expect(f.locator(".rail")).toHaveCount(0);
  await expect(f.locator(".image-id")).not.toContainText("1 of 1");
  // nothing to choose between: keep or drop only, unless the payload asks for a favourite
  await expect(f.locator('.choice[data-action="favorite"]')).toHaveCount(0);
  await expect(f.locator(".tally")).not.toContainText("favourite");
  const sheet = (await f.locator("#sheet").boundingBox())!;
  expect(sheet.x).toBeLessThan(5);

  review.payload.favorite = true;
  const asked = await mountPlugin(page, dir, { review });
  await expect(asked.frame.locator('.choice[data-action="favorite"]')).toHaveCount(1);
});

test("a note left empty goes, and markdown in one shows as text", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  await ready(plugin);
  await f.getByRole("button", { name: "Add a note" }).click();
  await f.locator("#note").fill("   ");
  await f.locator("#note").press("Escape");
  await expect(f.getByRole("button", { name: "Add a note" })).toBeVisible();
  await f.getByRole("button", { name: "Add a note" }).click();
  await f.locator("#note").fill("Make the sun **warmer**");
  await f.locator("#note").press("Escape");
  await expect(f.locator(".image-note strong")).toHaveText("warmer");
  await expect(f.getByRole("button", { name: "Add a note" })).toHaveCount(0);
});

test("a round without a favourite keeps or drops each image on its own", async ({ page }) => {
  const review = round();
  review.payload.favorite = false;
  const plugin = await mountPlugin(page, dir, { review });
  const f = plugin.frame;
  await ready(plugin);
  await expect(f.locator('.choice[data-action="favorite"]')).toHaveCount(0);
  await expect(f.locator(".tally")).not.toContainText("favourite");
  await f.locator("body").click({ position: { x: 600, y: 5 } });
  await f.locator("body").press("f"); // nothing
  await expect(f.locator(".pick").nth(0).locator(".verdict-chip")).not.toContainText("Favourite");
  await f.locator("body").press("s");
  await expect(f.locator(".pick").nth(0).locator(".verdict-chip")).toHaveText("Keep");
});

test("a decision the app refuses says why in the confirmation bar", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  await ready(plugin);
  await plugin.sendViolations([{ path: "/decisions/0/note", message: "is too long" }]);
  const bar = f.locator(".pinrail-confirmation");
  await expect(bar).toHaveClass(/pinrail-confirmation-danger/);
  await expect(bar).toContainText("/decisions/0/note: is too long");
  await bar.getByRole("button", { name: "Keep deciding" }).click();
  await expect(bar).toBeHidden();
});

test("an image's note has edit and delete at its top right", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  await ready(plugin);
  await f.getByRole("button", { name: "Add a note" }).click();
  await f.locator("#note").fill("Warmer light");
  await f.locator("#note").press("Enter");
  const note = f.locator(".image-note");
  await note.getByRole("button", { name: "Edit the note" }).click();
  await expect(f.locator("#note")).toHaveValue("Warmer light");
  await f.locator("#note").press("Enter");
  await f.locator(".image-note").getByRole("button", { name: "Delete the note" }).click();
  await expect(f.locator(".image-note")).toHaveCount(0);
  await expect(f.getByRole("button", { name: "Add a note" })).toBeVisible();
});

test("an area's comment can be edited and deleted from the list, and a click on it edits it", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  await ready(plugin);
  await page.mouse.click(...(await onImage(plugin, 0.875, 0.3)));
  await f.locator("#region-note").fill("Nose up");
  await f.locator("#region-note").press("Enter");
  const item = f.locator(".regions li").first();
  await expect(item).toContainText("Nose up");

  // the pencil opens the area's comment again, with its text, and saving replaces it
  await item.getByRole("button", { name: "Edit the comment" }).click();
  await expect(f.locator("#pop")).toContainText("Point");
  await expect(f.locator("#region-note")).toHaveValue("Nose up");
  await f.locator("#region-note").fill("Nose up a little");
  await f.locator("#region-note").press("Enter");
  await expect(f.locator(".regions li")).toHaveCount(1);
  await expect(item).toContainText("Nose up a little");

  // a click on the comment edits it too
  await item.locator(".region-text").click();
  await expect(f.locator("#region-note")).toHaveValue("Nose up a little");
  await f.locator("#region-note").press("Escape");
  await expect(item).toContainText("Nose up a little");

  await item.getByRole("button", { name: "Delete the comment" }).click();
  await expect(f.locator(".regions li")).toHaveCount(0);
});

test("a dragged box and a clicked point go back in fractions and in pixels", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  await ready(plugin);

  await page.mouse.move(...(await onImage(plugin, 0.25, 0.5)));
  await page.mouse.down();
  await page.mouse.move(...(await onImage(plugin, 0.5, 0.75)), { steps: 5 });
  await page.mouse.up();
  await expect(f.locator("#pop")).toContainText("Box");
  await f.locator("#region-note").fill("Remove the stray trail");
  await f.locator("#region-note").press("Enter");

  await page.mouse.click(...(await onImage(plugin, 0.875, 0.3)));
  await expect(f.locator("#pop")).toContainText("Point");
  await f.locator("#region-note").fill("Nose up");
  await f.locator('[data-pop="add"]').click();

  await expect(f.locator(".regions li")).toHaveCount(2);
  await expect(f.locator(".marks .region")).toHaveCount(1);
  await expect(f.locator(".marks .pin")).toHaveCount(1);
  await expect(f.locator(".pick").nth(0).locator(".verdict-chip")).toHaveText("Keep"); // regions keep it
  await expect(f.locator(".pick").nth(0).locator(".count")).toHaveText("2");

  await plugin.collect();
  await plugin.collect();
  const { decisions } = valid(await plugin.nextSubmit());
  expect(decisions).toEqual([
    {
      id: "A",
      action: "keep",
      size: { width: 960, height: 720 },
      regions: [
        {
          shape: "box",
          x: expect.closeTo(0.25, 2),
          y: expect.closeTo(0.5, 2),
          width: expect.closeTo(0.25, 2),
          height: expect.closeTo(0.25, 2),
          px: { x: near(240), y: near(360), width: near(240), height: near(180) },
          note: "Remove the stray trail",
        },
        {
          shape: "point",
          x: expect.closeTo(0.875, 2),
          y: expect.closeTo(0.3, 2),
          px: { x: near(840), y: near(216) },
          note: "Nose up",
        },
      ],
    },
  ]);
  for (const r of decisions[0].regions)
    for (const v of Object.values(r.px) as number[]) expect(Number.isInteger(v)).toBe(true);
});

test("a box stays inside the image, and a click off the image asks nothing", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  await ready(plugin);
  const stage = (await f.locator(".stage").boundingBox())!;
  await page.mouse.click(stage.x + 30, stage.y + stage.height / 2);
  await expect(f.locator("#pop")).toHaveCount(0);

  await page.mouse.move(...(await onImage(plugin, 0.8, 0.8)));
  await page.mouse.down();
  await page.mouse.move(stage.x + stage.width - 5, stage.y + stage.height - 5, { steps: 5 });
  await page.mouse.up();
  await f.locator("#region-note").fill("Corner");
  await f.locator("#region-note").press("Enter");
  await plugin.collect();
  await plugin.collect();
  const [r] = valid(await plugin.nextSubmit()).decisions[0].regions;
  expect(r.x + r.width).toBeLessThanOrEqual(1);
  expect(r.y + r.height).toBeLessThanOrEqual(1);
  expect(r.px.x + r.px.width).toBeLessThanOrEqual(960);
});

test("from the keyboard: zoom, a pin at the centre, and a box around what is in view", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  const f = plugin.frame;
  await ready(plugin);
  await f.locator(".stage").focus();
  await f.locator(".stage").press("m");
  await expect(f.locator("#pop")).toContainText("Point");
  await f.locator("#region-note").fill("Centre");
  await f.locator("#region-note").press("Enter");

  await f.locator(".stage").press("b"); // the whole image is in view
  await expect(f.locator(".hint")).toContainText("Zoom in on a part first");
  await expect(f.locator("#pop")).toHaveCount(0);

  const before = await f.locator(".pct").textContent();
  await f.locator(".stage").press("=");
  await f.locator(".stage").press("=");
  await f.locator(".stage").press("=");
  await expect(f.locator(".pct")).not.toHaveText(before!);
  await f.locator(".stage").press("b");
  await expect(f.locator("#pop")).toContainText("Box");
  await f.locator("#region-note").fill("This part");
  await f.locator("#region-note").press("Enter");
  await f.locator(".stage").press("0");
  await expect(f.locator(".pct")).toHaveText(before!);

  await plugin.collect();
  await plugin.collect();
  const [point, box] = valid(await plugin.nextSubmit()).decisions[0].regions;
  expect(point).toMatchObject({ shape: "point", x: expect.closeTo(0.5, 1), y: expect.closeTo(0.5, 1) });
  expect(box.shape).toBe("box");
  expect(box.width).toBeLessThan(0.8);
  expect(box.x + box.width / 2).toBeCloseTo(0.5, 1);
});

test("a draft comes back with its verdicts, notes and regions", async ({ page }) => {
  let plugin = await mountPlugin(page, dir, { review: round() });
  let f = plugin.frame;
  await ready(plugin);
  await page.mouse.click(...(await onImage(plugin, 0.5, 0.84)));
  await f.locator("#region-note").fill("Ground the post");
  await f.locator("#region-note").press("Enter");
  await f.locator('.choice[data-action="favorite"]').click();
  await f.getByRole("button", { name: "Add a note" }).click();
  await f.locator("#note").fill("Warmer sky");
  await expect.poll(async () => (await plugin.lastDraft())?.A?.note).toBe("Warmer sky");
  const draft = await plugin.lastDraft();

  plugin = await mountPlugin(page, dir, { review: round(), draft });
  f = plugin.frame;
  await ready(plugin);
  await expect(f.locator(".regions li")).toHaveCount(1);
  await expect(f.locator(".regions li")).toContainText("Ground the post");
  await expect(f.locator(".marks .pin")).toHaveCount(1);
  // a written note shows as text, and a click on it edits it
  await expect(f.locator(".image-note")).toHaveText("Warmer sky");
  await f.locator(".image-note").click();
  await expect(f.locator("#note")).toHaveValue("Warmer sky");
  await expect(f.locator(".pick").nth(0).locator(".verdict-chip")).toHaveText("★ Favourite");
});

test("a decided round is read-only and shows what was decided", async ({ page }) => {
  const review = fixture(path.join(dir, "fixtures", "tern.decided.json"));
  const plugin = await mountPlugin(page, dir, { review, readonly: true });
  const f = plugin.frame;
  await ready(plugin);
  await expect(f.locator(".pick").nth(0).locator(".verdict-chip")).toHaveText("★ Favourite");
  await expect(f.locator(".pick").nth(2).locator(".verdict-chip")).toHaveText("Drop");
  await expect(f.locator('.choice[data-action="keep"]')).toBeDisabled();
  await expect(f.locator("#note")).toHaveCount(0);
  await expect(f.getByRole("button", { name: "Add a note" })).toHaveCount(0);
  await expect(f.locator(".regions li")).toHaveCount(3);
  await expect(f.locator(".regions li").first()).toContainText("box 872,296 · 88×100 px");
  await expect(f.locator(".marks .region")).toHaveCount(2);
  await expect(f.locator("[data-remove]")).toHaveCount(0);
  // a click pans rather than asks
  await page.mouse.click(...(await onImage(plugin, 0.5, 0.5)));
  await expect(f.locator("#pop")).toHaveCount(0);
});

test("an image that cannot be read says so, and the others still show", async ({ page }) => {
  const review = round();
  const broken = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "image-")), "broken.png");
  fs.writeFileSync(broken, "not an image");
  review.payload.images[0].file = { $attachment: "broken.png" };
  const plugin = await mountPlugin(page, dir, {
    review,
    attachments: { ...files(["mailbox.jpg", "hammock.webp", "tern.svg"]), "broken.png": broken },
  });
  const f = plugin.frame;
  await expect(f.locator(".broken")).toContainText("could not be read");
  await expect(f.locator(".pick .thumb img")).toHaveCount(3);
  await expect(f.locator(".pick").nth(0)).toContainText("can't read it");
  // it can still be dropped, and the rest reviewed
  await f.locator('.choice[data-action="drop"]').click();
  await f.locator(".pick").nth(1).click();
  await expect(f.locator(".stage .art")).toBeVisible();
});

test("an SVG is drawn as a picture: its scripts never run", async ({ page }) => {
  const review = round();
  const evil = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "image-")), "evil.svg");
  fs.writeFileSync(
    evil,
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" onload="parent.pwned = 1">
    <script>parent.pwned = 2; top.pwned = 2;</script>
    <rect x="4" y="4" width="16" height="16" fill="teal"/></svg>`,
  );
  review.payload.images = [{ id: "E", name: "Icon", file: { $attachment: "evil.svg" } }];
  const plugin = await mountPlugin(page, dir, { review, attachments: { "evil.svg": evil } });
  const f = plugin.frame;
  await expect(f.locator(".stage .art")).toBeVisible();
  await expect(f.locator(".foot")).toContainText("24 × 24 · SVG");
  await page.mouse.click(...(await onImage(plugin, 0.5, 0.25)));
  await f.locator("#region-note").fill("Round the corners");
  await f.locator("#region-note").press("Enter");
  expect(await f.locator("body").evaluate(() => (window as any).pwned)).toBeUndefined();
  expect(await page.evaluate(() => (window as any).pwned)).toBeUndefined();
  await plugin.collect();
  const d = valid(await plugin.nextSubmit());
  // an SVG's pixels are its viewBox units, in hundredths on a small drawing
  expect(d.decisions[0]).toMatchObject({
    size: { width: 24, height: 24 },
    regions: [{ shape: "point", px: { x: expect.closeTo(12, 0), y: expect.closeTo(6, 0) } }],
  });
});

test("a second round compares with the first, and its regions", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, {
    review: fixture(path.join(dir, "fixtures", "tern-round-2.json")),
    previous: fixture(path.join(dir, "fixtures", "tern.decided.json")),
  });
  const f = plugin.frame;
  await expect(f.locator(".stage .art")).toBeVisible();
  await expect(f.locator(".previous")).toContainText("3 regions asked for");
  const current = await f.locator(".stage .art").getAttribute("src");
  await f.locator(".stage").press("c");
  await expect(f.locator(".banner")).toHaveText("Previous round · 3 regions asked for");
  await expect(f.locator(".marks .past")).toHaveCount(3);
  await expect(f.locator(".stage .art")).not.toHaveAttribute("src", current!);
  const asked = (await plugin.messages())
    .filter((m: any) => m.type === "attachment" && m.round === "previous")
    .map((m: any) => m.name);
  expect(asked).toEqual(["paper-plane.png"]);
  // nothing is marked on the previous round
  await page.mouse.click(...(await onImage(plugin, 0.5, 0.5)));
  await expect(f.locator("#pop")).toHaveCount(0);
  await f.locator(".stage").press("c");
  await expect(f.locator(".banner")).toHaveCount(0);
});

test("in a narrow frame the stage's bars do not overlap, and the verdicts keep one row", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 800 });
  const plugin = await mountPlugin(page, dir, { review: round() });
  await ready(plugin);
  const f = plugin.frame;
  const zoom = (await f.getByRole("group", { name: "Zoom" }).boundingBox())!;
  const tools = (await f.getByRole("group", { name: "Stage" }).boundingBox())!;
  const apart = zoom.x + zoom.width <= tools.x || tools.x + tools.width <= zoom.x || zoom.y >= tools.y + tools.height;
  expect(apart).toBe(true);
  const tops = await f
    .locator(".image-head .choice")
    .evaluateAll((buttons) => buttons.map((b) => Math.round(b.getBoundingClientRect().top)));
  expect(new Set(tops).size).toBe(1);
  // the tooltip still shows the button's shortcut
  await expect(f.locator('.choice[data-action="keep"]')).toHaveAttribute("title", "Keep (s)");
});

test("the round's notes fold away to a line, and the view keeps the choice", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  await ready(plugin);
  const f = plugin.frame;
  const notes = f.locator("details.notes");
  await expect(notes).toHaveAttribute("open", "");
  await notes.locator("summary").click();
  await expect(notes.locator(".notes-body")).toBeHidden();
  await expect(notes.locator(".notes-peek")).toContainText("Round 1 of the empty inbox");
  await expect.poll(() => plugin.lastSettingsSet()).toEqual({ notes_open: false });
  // a redraw keeps the notes folded, and a later setting still applies
  await f.locator('.choice[data-action="keep"]').click();
  await expect(notes.locator(".notes-body")).toBeHidden();
  await plugin.settings({ notes_open: true });
  await expect(notes.locator(".notes-body")).toBeVisible();
});

test("a review opened with the notes folded starts with them folded", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round(), settings: { notes_open: false } });
  await ready(plugin);
  await expect(plugin.frame.locator("details.notes .notes-body")).toBeHidden();
});

test("in a short, narrow frame the view scrolls as one, down to the image", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 640 });
  const plugin = await mountPlugin(page, dir, { review: round() });
  await ready(plugin);
  const f = plugin.frame;
  const stage = f.locator(".stage");
  expect((await stage.boundingBox())!.height).toBeGreaterThanOrEqual(280);
  await stage.scrollIntoViewIfNeeded();
  await expect(f.locator(".stage .art")).toBeInViewport();
  // a verdict redraws the view and keeps the scroll position
  const before = await f.locator("#app").evaluate((el) => el.scrollTop);
  expect(before).toBeGreaterThan(0);
  await f.locator('.choice[data-action="keep"]').click();
  expect(await f.locator("#app").evaluate((el) => el.scrollTop)).toBe(before);
});

test("an image's note and a region's comment can be dictated", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round(), dictation: true });
  const f = plugin.frame;
  await ready(plugin);

  await f.getByRole("button", { name: "Add a note" }).click();
  await f.locator(".pinrail-compose:has(#note)").getByRole("button", { name: "Dictate" }).click();
  await plugin.say("warmer");
  await plugin.say("warmer light overall");
  await expect(f.locator("#note")).toHaveValue("warmer light overall");
  await f.getByRole("button", { name: "Stop dictating" }).click();
  await expect.poll(() => plugin.dictating()).toBe(false);

  await page.mouse.click(...(await onImage(plugin, 0.875, 0.3)));
  await expect(f.locator("#pop")).toContainText("Point");
  await f.locator("#pop").getByRole("button", { name: "Dictate" }).click();
  await plugin.say("nose up a little");
  await expect(f.locator("#region-note")).toHaveValue("nose up a little");
  await f.locator('[data-pop="add"]').click();
  await expect(f.locator(".regions li")).toContainText("nose up a little");
});

test("without the app's dictation the note has no microphone", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: round() });
  await ready(plugin);
  await plugin.frame.getByRole("button", { name: "Add a note" }).click();
  await expect(plugin.frame.locator("#note")).toBeVisible();
  await expect(plugin.frame.getByRole("button", { name: "Dictate" })).toHaveCount(0);
});

test("in a narrow but short frame the stage still takes the full width", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  const plugin = await mountPlugin(page, dir, { review: round() });
  await ready(plugin);
  const f = plugin.frame;
  const stage = (await f.locator(".stage").boundingBox())!;
  const sheet = (await f.locator(".sheet-inner").boundingBox())!;
  // the stage spans the sheet's content area, apart from its padding
  expect(stage.width).toBeGreaterThan(sheet.width - 80);
});

test("a hand-over before the images are read waits for them and keeps the draft's regions", async ({ page }) => {
  let plugin = await mountPlugin(page, dir, { review: round() });
  await ready(plugin);
  await page.mouse.click(...(await onImage(plugin, 0.5, 0.84)));
  await plugin.frame.locator("#region-note").fill("Ground the post");
  await plugin.frame.locator("#region-note").press("Enter");
  await expect.poll(async () => (await plugin.lastDraft())?.A?.regions?.length).toBe(1);
  // the other images decided, so the hand-over asks nothing more
  const draft = { ...(await plugin.lastDraft()), B: { action: "drop" }, C: { action: "drop" }, D: { action: "drop" } };

  // the images are held back until the hand-over has been asked for
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route("**/_attachments/**", async (route) => {
    await held;
    await route.fallback();
  });
  plugin = await mountPlugin(page, dir, { review: round(), draft });
  const collecting = plugin.collect();
  setTimeout(release, 300);
  await collecting;
  const decision = valid(await plugin.nextSubmit());
  expect(decision.decisions[0].regions).toHaveLength(1);
});

test("a region note typed and not yet added goes with the hand-over", async ({ page }) => {
  // the other images decided, so the hand-over asks nothing more
  const draft = { B: { action: "drop" }, C: { action: "drop" }, D: { action: "drop" } };
  const plugin = await mountPlugin(page, dir, { review: round(), draft });
  await ready(plugin);
  await page.mouse.click(...(await onImage(plugin, 0.5, 0.84)));
  await plugin.frame.locator("#region-note").fill("Ground the post");
  await plugin.collect();
  const decision = valid(await plugin.nextSubmit());
  expect(decision.decisions[0].regions.map((r: { note: string }) => r.note)).toEqual(["Ground the post"]);
});
