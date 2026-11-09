import { expect, test } from "@playwright/test";
import path from "node:path";
import { fixture, mountPlugin } from "pinrail-sdk/testing";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import { layout, routePath } from "../src/layout";
import { coverage } from "../src/lines";

// The view alone, under the harness: no app, no CLI. `mountPlugin` serves
// this folder as the app would and plays the app's side of the protocol.
const dir = path.resolve(__dirname, "..");
const dedup = () => fixture(path.join(dir, "samples", "dedup-tickets.json"));

for (const drawer of [false, true])
  test(`a part chosen in the overview's list is zoomed into on the map, ${drawer ? "with" : "without"} the drawer`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 860 });
    const plugin = await mountPlugin(page, dir, { review: dedup(), settings: { show_diff: drawer } });
    const f = plugin.frame;
    await f.locator(".cm-part").first().waitFor();
    const scale = () =>
      f.locator(".react-flow__viewport").evaluate((el) => new DOMMatrix(getComputedStyle(el).transform).a);
    const fitted = await scale();
    await f
      .getByTestId("overview")
      .getByRole("button", { name: /^Tickets/ })
      .click();
    // the map zooms in, as close as a fit goes, with the part in the middle of what is in view
    await expect.poll(scale).toBeCloseTo(1.15, 2);
    expect(fitted).toBeLessThan(1.15);
    const map = (await f.locator(".cm-diagram").boundingBox())!;
    // above the drawer, when the selection opened it
    await expect(f.getByTestId("diff")).toHaveCount(drawer ? 1 : 0);
    const bottom = drawer ? (await f.getByTestId("diff").boundingBox())!.y : map.y + map.height;
    const off = async () => {
      const part = (await f.locator('[data-node="tickets-context"]').boundingBox())!;
      return Math.max(
        Math.abs(part.x + part.width / 2 - (map.x + map.width / 2)),
        Math.abs(part.y + part.height / 2 - (map.y + bottom) / 2),
      );
    };
    await expect.poll(off).toBeLessThan(4);
  });

test("detailed cards, the default, head each part with its kind and status and count its lines", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  const form = f.locator('[data-node="ticket-form"]');
  await expect(form.locator(".cm-part-band")).toHaveText(/Interface\s*changed/);
  // the header and the body take the card's whole width, in WebKit too,
  // where a button's content does not stretch by itself
  const card = (await form.boundingBox())!;
  for (const part of [".cm-part-band", ".cm-part-body"])
    expect((await form.locator(part).boundingBox())!.width).toBeGreaterThan(card.width - 4);
  // and the screenshot stays inside it, in both styles
  for (const style of ["detailed", "compact"]) {
    await plugin.settings({ card_style: style });
    const box = (await form.boundingBox())!;
    const shot = (await form.locator(".cm-part-shot").boundingBox())!;
    expect(shot.x, style).toBeGreaterThan(box.x);
    expect(shot.x + shot.width, style).toBeLessThan(box.x + box.width);
    expect(shot.y + shot.height, style).toBeLessThan(box.y + box.height);
  }
  await plugin.settings({ card_style: "detailed" });
  // the lines its range 40-68 of TicketForm.tsx adds and removes
  await expect(form.locator(".cm-part-lines")).toHaveText("+15−3");
  // a part that is a whole file counts the file
  await expect(f.locator('[data-node="api-client"] .cm-part-lines')).toHaveText("+9−3");
  // a part shown for context has no lines of its own
  await expect(f.locator('[data-node="mailer"] .cm-part-band')).toContainText("unchanged");
  await expect(f.locator('[data-node="mailer"] .cm-part-lines')).toHaveCount(0);
  // a comment's badge stays on the card's top-right corner
  await form.click();
  await f.getByPlaceholder("Add a comment").fill("Keep the key after a failed save.");
  await f.getByPlaceholder("Add a comment").press("Enter");
  const badge = (await form.locator(".cm-marks-corner").boundingBox())!;
  expect(badge.y).toBeLessThan(card.y);
  expect(badge.x + badge.width).toBeGreaterThan(card.x + card.width);
});

test("the cards button on the map switches to the other style, and the choice is a setting", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await expect(f.locator(".cm-part-band").first()).toBeVisible();
  // the button says what a click does, and is never shown pressed
  await f.getByRole("button", { name: "Show compact cards" }).click();
  await expect(f.locator(".cm-part-band")).toHaveCount(0);
  await expect(f.locator('[data-node="ticket-form"] .cm-status')).toHaveText("changed");
  const back = f.getByRole("button", { name: "Show detailed cards" });
  await expect(back).not.toHaveAttribute("aria-pressed");
  await expect.poll(() => plugin.lastSettingsSet()).toMatchObject({ card_style: "compact" });
  await back.click();
  await expect(f.locator('[data-node="ticket-form"] .cm-part-band')).toBeVisible();
  await expect.poll(() => plugin.lastSettingsSet()).toMatchObject({ card_style: "detailed" });
  // the app's settings set it too
  await plugin.settings({ card_style: "compact" });
  await expect(f.locator(".cm-part-band")).toHaveCount(0);
  await expect(back).toBeVisible();
});

for (const style of ["detailed", "compact"])
  test(`a selected ${style} card has a border in the accent colour`, async ({ page }) => {
    const plugin = await mountPlugin(page, dir, { review: dedup(), settings: { card_style: style } });
    const f = plugin.frame;
    const form = f.locator('[data-node="ticket-form"]');
    const accent = await form.evaluate((el) => {
      const probe = document.createElement("span");
      probe.style.color = "var(--pinrail-accent)";
      el.ownerDocument.body.append(probe);
      const color = getComputedStyle(probe).color;
      probe.remove();
      return color;
    });
    const border = () => form.evaluate((el) => getComputedStyle(el).borderTopColor);
    expect(await border()).not.toBe(accent);
    await form.click();
    await expect.poll(border).toBe(accent);
  });

test("a compact card's status line starts at the card's left, under its icon", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup(), settings: { card_style: "compact" } });
  const f = plugin.frame;
  const form = f.locator('[data-node="ticket-form"]');
  const icon = (await form.locator(".cm-part-icon").boundingBox())!;
  const meta = (await form.locator(".cm-part-meta > *").first().boundingBox())!;
  expect(Math.abs(meta.x - icon.x)).toBeLessThan(1);
});

test("each interaction's label sits on its line", async ({ page }) => {
  await page.setViewportSize({ width: 1600, height: 1000 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator(".cm-part").first().waitFor();
  // how far each label's middle is from the nearest point of its drawn line
  const off = await f.locator("body").evaluate(() =>
    [...document.querySelectorAll(".react-flow__edge")].map((g) => {
      const line = g.querySelector<SVGPathElement>("path.cm-link")!;
      const id = g.getAttribute("data-id");
      const label = document.querySelector(`.cm-link-label[data-edge="${id}"]`)!.getBoundingClientRect();
      const m = line.getScreenCTM()!;
      const length = line.getTotalLength();
      let nearest = Infinity;
      for (let i = 0; i <= 400; i++) {
        const p = line.getPointAtLength((length * i) / 400);
        nearest = Math.min(
          nearest,
          Math.hypot(p.x * m.a + m.e - (label.x + label.width / 2), p.y * m.d + m.f - (label.y + label.height / 2)),
        );
      }
      return { id, nearest: Math.round(nearest * 10) / 10 };
    }),
  );
  expect(off.length).toBeGreaterThan(3);
  for (const { id, nearest } of off) expect(nearest, `the label of ${id}`).toBeLessThan(1.5);
});

test("each interaction's line passes through its label and stays within the corners of its route", async ({ page }) => {
  const payload = JSON.parse(fs.readFileSync(path.join(dir, "samples", "dedup-tickets.json"), "utf8")).payload;
  const routes = [...layout(payload, "detailed").edges.values()];
  expect(routes.length).toBeGreaterThan(3);
  await page.setContent("<svg width='2000' height='2000'><path id='line' fill='none' stroke='black'/></svg>");
  for (const route of routes) {
    const d = routePath(route.points);
    const { box, nearest } = await page.locator("#line").evaluate(
      (el: SVGPathElement, [d, label]) => {
        el.setAttribute("d", d as string);
        const { x, y } = label as { x: number; y: number };
        const length = el.getTotalLength();
        let nearest = Infinity;
        for (let i = 0; i <= 400; i++) {
          const p = el.getPointAtLength((length * i) / 400);
          nearest = Math.min(nearest, Math.hypot(p.x - x, p.y - y));
        }
        const b = el.getBBox();
        return { box: { left: b.x, top: b.y, right: b.x + b.width, bottom: b.y + b.height }, nearest };
      },
      [d, route.label] as const,
    );
    const xs = route.points.map((p) => p.x);
    const ys = route.points.map((p) => p.y);
    expect(nearest, d).toBeLessThan(0.5);
    expect(box.left, d).toBeGreaterThanOrEqual(Math.min(...xs) - 0.5);
    expect(box.right, d).toBeLessThanOrEqual(Math.max(...xs) + 0.5);
    expect(box.top, d).toBeGreaterThanOrEqual(Math.min(...ys) - 0.5);
    expect(box.bottom, d).toBeLessThanOrEqual(Math.max(...ys) + 0.5);
  }
});

for (const style of ["detailed", "compact"])
  test(`no interaction's label covers a part or another label, with ${style} cards`, async ({ page }) => {
    await page.setViewportSize({ width: 1600, height: 1000 });
    // the map of a change to this plugin, where a label once sat on a card
    const map = fixture(path.join(dir, "tests", "maps", "change-map-itself.json"));
    const plugin = await mountPlugin(page, dir, { review: map, settings: { card_style: style } });
    const f = plugin.frame;
    await f.locator(".cm-part").first().waitFor();
    const covered = await f.locator("body").evaluate(() => {
      const cards = [...document.querySelectorAll(".cm-part")].map((c) => ({
        id: c.getAttribute("data-node"),
        box: c.getBoundingClientRect(),
      }));
      const labels = [...document.querySelectorAll(".cm-link-label")].map((l) => ({
        id: l.getAttribute("data-edge"),
        box: l.getBoundingClientRect(),
      }));
      const meet = (r: DOMRect, c: DOMRect) =>
        r.left < c.right && r.right > c.left && r.top < c.bottom && r.bottom > c.top;
      return labels.flatMap(({ id, box }, i) => [
        ...cards.filter((c) => meet(box, c.box)).map((c) => `${id} covers ${c.id}`),
        ...labels
          .slice(i + 1)
          .filter((l) => meet(box, l.box))
          .map((l) => `${id} covers ${l.id}`),
      ]);
    });
    expect(covered).toEqual([]);
  });

test("a state diagram lays out left to right or top to bottom, and the choice is a setting", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-tab="save-button"]').click();
  const centre = async (id: string) => {
    const b = (await f.locator(`[data-item="${id}"]`).boundingBox())!;
    return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
  };
  // left to right at first: Ready follows the start to its right
  let [start, ready] = [await centre("start"), await centre("ready")];
  expect(ready.x - start.x).toBeGreaterThan(Math.abs(ready.y - start.y));
  // the button says what a click does
  await f.getByRole("button", { name: "Lay out top to bottom" }).click();
  await expect
    .poll(async () => {
      [start, ready] = [await centre("start"), await centre("ready")];
      return ready.y - start.y > Math.abs(ready.x - start.x);
    })
    .toBe(true);
  await expect.poll(() => plugin.lastSettingsSet()).toMatchObject({ state_direction: "TB" });
  await f.getByRole("button", { name: "Lay out left to right" }).click();
  await expect.poll(() => plugin.lastSettingsSet()).toMatchObject({ state_direction: "LR" });
  // the app's settings set it too
  await plugin.settings({ state_direction: "TB" });
  await expect(f.getByRole("button", { name: "Lay out left to right" })).toBeVisible();
});

for (const direction of ["LR", "TB"])
  test(`no label covers a state or another label, with the state diagram ${direction === "LR" ? "left to right" : "top to bottom"}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1400, height: 900 });
    const plugin = await mountPlugin(page, dir, { review: dedup(), settings: { state_direction: direction } });
    const f = plugin.frame;
    await f.locator('[data-tab="save-button"]').click();
    await f.locator('[data-item="ready"]').waitFor();
    const covered = await f.locator(".cm-diagram").evaluate((root) => {
      const states = [...root.querySelectorAll("[data-item]")]
        .filter((el) => el.matches(".cm-part, .cm-state-dot"))
        .map((el) => ({ id: el.getAttribute("data-item"), box: el.getBoundingClientRect() }));
      const labels = [...root.querySelectorAll(".cm-link-label")].map((el) => ({
        id: el.textContent,
        box: el.getBoundingClientRect(),
      }));
      const meet = (r: DOMRect, c: DOMRect) =>
        r.left < c.right && r.right > c.left && r.top < c.bottom && r.bottom > c.top;
      return labels.flatMap(({ id, box }, i) => [
        ...states.filter((s) => meet(box, s.box)).map((s) => `${id} covers ${s.id}`),
        ...labels
          .slice(i + 1)
          .filter((l) => meet(box, l.box))
          .map((l) => `${id} covers ${l.id}`),
      ]);
    });
    expect(covered).toEqual([]);
  });

test("a state diagram shows each state's name whole, beside its files", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-tab="save-button"]').click();
  for (const id of ["ready", "saving"]) {
    const title = f.locator(`[data-item="${id}"] .cm-part-title`);
    await expect(title).toBeVisible();
    expect(await title.evaluate((el) => el.scrollWidth <= el.clientWidth), id).toBe(true);
  }
});

test("an interaction's label shows how many files it has, on the map and in the diagrams", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  // on the map
  await expect(f.locator('.cm-link-label[data-edge="save"] .cm-link-files')).toHaveText("1");
  await expect(f.locator('.cm-link-label[data-edge="save"]')).toHaveAttribute("aria-label", /1 file/);
  // a message of a sequence diagram, with files of its own, or those of the
  // interaction it stands for
  await f.locator('[data-tab="first-save"]').click();
  await expect(f.locator('[data-item="m5"] .cm-link-files')).toHaveText("1");
  await expect(f.locator('[data-item="m1"] .cm-link-files')).toHaveText("1");
  // a transition of a state diagram
  await f.locator('[data-tab="save-button"]').click();
  await expect(f.locator('.cm-link-label[data-item="t2"] .cm-link-files')).toHaveText("1");
});

test("a map whose interactions have no files shows no file counts on them", async ({ page }) => {
  const map = fixture(path.join(dir, "tests", "maps", "change-map-itself.json"));
  const plugin = await mountPlugin(page, dir, { review: map });
  const f = plugin.frame;
  await expect(f.locator(".cm-link-label").first()).toBeVisible();
  await expect(f.locator(".cm-link-files")).toHaveCount(0);
});

test("the drawer names an interaction without files in monospace, as its label reads", async ({ page }) => {
  const map = fixture(path.join(dir, "tests", "maps", "change-map-itself.json"));
  const plugin = await mountPlugin(page, dir, { review: map, settings: { show_diff: true } });
  const f = plugin.frame;
  await f.locator('.cm-link-label[data-edge="app-types"]').click();
  const name = f.getByTestId("diff").locator(".cm-diff-owner");
  // the brackets at its end stay at its end
  await expect(name).toHaveText("narrowest() · diagramComments()");
  expect(await name.evaluate((el) => getComputedStyle(el).direction)).toBe("ltr");
  const mono = await name.evaluate((el) => getComputedStyle(el).getPropertyValue("--pinrail-mono").trim());
  expect(await name.evaluate((el) => getComputedStyle(el).fontFamily)).toBe(mono);
});

test("the drawer names a state without files in the text's font", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup(), settings: { show_diff: true } });
  const f = plugin.frame;
  await f.locator('[data-tab="save-button"]').click();
  await f.locator('[data-item="start"]').click();
  const name = f.getByTestId("diff").locator(".cm-diff-owner");
  await expect(name).toHaveText("Start");
  const mono = await name.evaluate((el) => getComputedStyle(el).getPropertyValue("--pinrail-mono").trim());
  expect(await name.evaluate((el) => getComputedStyle(el).fontFamily)).not.toBe(mono);
});

test("a selection without files shows an icon and a light line above what is missing, the same each time", async ({
  page,
}) => {
  const map = fixture(path.join(dir, "tests", "maps", "change-map-itself.json"));
  const plugin = await mountPlugin(page, dir, { review: map, settings: { show_diff: true } });
  const f = plugin.frame;
  const empty = f.getByTestId("no-files");
  await f.locator('.cm-link-label[data-edge="app-types"]').click();
  await expect(empty.locator("svg")).toBeVisible();
  const quip = (await empty.locator(".cm-empty-quip").textContent())!;
  expect(quip.length).toBeGreaterThan(10);
  await expect(empty).toContainText("No files of narrowest() · diagramComments() are in this change.");
  // another interaction, then this one again: the same line. The drawer
  // may cover their labels, so they are clicked without the pointer.
  await f.locator('.cm-link-label[data-edge="seq-diagram"]').dispatchEvent("click");
  await expect(empty).toContainText("No files of Fitter · Follower · Marked");
  await f.locator('.cm-link-label[data-edge="app-types"]').dispatchEvent("click");
  await expect(empty.locator(".cm-empty-quip")).toHaveText(quip);
});

test("the skill's check notes what a map leaves out, and passes a map that leaves out nothing", () => {
  const check = (payload: unknown) => {
    const file = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "cm-check-")), "map.json");
    fs.writeFileSync(file, JSON.stringify(payload));
    return execFileSync("node", [path.join(dir, "skill", "scripts", "check.mjs"), file], { encoding: "utf8" });
  };
  const sample = JSON.parse(fs.readFileSync(path.join(dir, "samples", "dedup-tickets.json"), "utf8")).payload;
  // the sample leaves only its changelog off the map, which belongs there
  expect(check(sample)).toBe("Nothing is left out.\n");
  // a lock file is not noted either, while a code file is
  const files = structuredClone(sample);
  files.files.push({ path: "package-lock.json", status: "modified", diff: "" });
  files.files.push({ path: "lib/helpdesk/forgotten.ex", status: "modified", diff: "" });
  expect(check(files)).toBe(
    ["Changed files no part, interaction or diagram names:", "  lib/helpdesk/forgotten.ex", ""].join("\n"),
  );
  // a changed part without lines, an interaction without lines, and lines
  // in a file the change does not touch
  const gaps = structuredClone(sample);
  delete gaps.nodes.find((n: { id: string }) => n.id === "api-client").changes;
  delete gaps.edges.find((e: { id: string }) => e.id === "post").changes;
  gaps.nodes.find((n: { id: string }) => n.id === "tickets-table").changes.push({ file: "lib/elsewhere.ex" });
  const out = check(gaps);
  expect(out).toContain("New or changed parts and interactions without lines:\n  part api-client\n  interaction post");
  expect(out).toContain("Lines in files the change does not touch:\n  part tickets-table: lib/elsewhere.ex");
});

/** The sample with the ticket form's lines narrowed to 40-45, and every
 *  other mention of its file taken off the map: 12 of its changed lines,
 *  the 11 added at 46-53 and 62-64 and the 1 removed before 62, are then
 *  on no part, interaction or diagram. The 2 removed right after 45 are
 *  on the ticket form. */
function narrowed() {
  const review = JSON.parse(fs.readFileSync(path.join(dir, "samples", "dedup-tickets.json"), "utf8"));
  const form = "web/src/TicketForm.tsx";
  const walk = (x: unknown): void => {
    if (Array.isArray(x)) return x.forEach(walk);
    if (!x || typeof x !== "object") return;
    const o = x as { changes?: { file: string }[] };
    if (o.changes) o.changes = o.changes.filter((c) => c.file !== form);
    Object.values(o).forEach(walk);
  };
  walk(review.payload);
  review.payload.nodes.find((n: { id: string }) => n.id === "ticket-form").changes = [
    { file: form, lines: [[40, 45]] },
  ];
  // the screenshots stay where the sample keeps them
  for (const entry of Object.values(review.attachments) as { path: string }[])
    entry.path = path.join(dir, "samples", entry.path);
  const file = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "cm-cover-")), "review.json");
  fs.writeFileSync(file, JSON.stringify(review));
  return fixture(file);
}

test("a line removed between two lines is on the map when a range reaches either of them", () => {
  // line 49 removed: it sat between what are lines 48 and 49 on the new side
  const diff = "@@ -47,4 +47,3 @@\n a\n b\n-gone\n c\n";
  const at = (lines: [number, number][]) =>
    coverage({
      change: { title: "t" },
      nodes: [{ id: "p", title: "P", changes: [{ file: "f.ts", lines }] }],
      files: [{ path: "f.ts", status: "modified", diff }],
    } as never).mapped;
  expect(at([[40, 48]])).toBe(1);
  expect(at([[49, 60]])).toBe(1);
  expect(at([[30, 47]])).toBe(0);
  expect(at([[50, 60]])).toBe(0);
});

test("the overview says how many changed lines are on the map", async ({ page }) => {
  let plugin = await mountPlugin(page, dir, { review: dedup() });
  await expect(plugin.frame.getByTestId("coverage")).toHaveText("Every changed line is on the map.");
  // 87 changed lines outside the changelog, 12 of them on nothing
  plugin = await mountPlugin(page, dir, { review: narrowed() });
  await expect(plugin.frame.getByTestId("coverage")).toHaveText("75 of 87 changed lines are on the map.");
});

test("the drawer marks the changed lines that nothing on the map covers", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  const plugin = await mountPlugin(page, dir, { review: narrowed(), settings: { show_diff: true } });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  const diff = f.getByTestId("diff");
  await expect(diff.locator(".cm-unmapped")).toHaveCount(12);
  // the lines removed right after the part's range are on it
  await expect(diff.locator('[data-line="old:42"]')).not.toHaveClass(/cm-unmapped/);
  await expect(diff.locator('[data-line="new:46"]')).toHaveClass(/cm-unmapped/);
  await expect(diff.locator('[data-line="new:43"]')).not.toHaveClass(/cm-unmapped/);
  await expect(diff.locator('[data-line="new:46"]')).toHaveAttribute("title", "Not on the map");
  await expect(diff.getByTestId("unmapped-count")).toHaveText("12 not on the map");
  // a file the map covers whole has none
  await f.locator('[data-node="api-client"]').click();
  await expect(diff.locator(".cm-unmapped")).toHaveCount(0);
  await expect(diff.getByTestId("unmapped-count")).toHaveCount(0);
});

test("the header counts what is reviewed, and U goes to the next part or interaction left to review", async ({
  page,
}) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  const progress = f.getByTestId("progress");
  // 5 parts and 5 interactions; the context part and the unchanged interaction do not count
  await expect(progress).toHaveText("0 of 10 reviewed");
  await f.locator('[data-node="ticket-form"]').click();
  await page.keyboard.press("r");
  await expect(progress).toHaveText("1 of 10 reviewed");
  // U: the first part not reviewed, after the parts the interactions
  await page.keyboard.press("u");
  await expect(f.locator('[data-node="api-client"]')).toHaveAttribute("aria-pressed", "true");
  for (const id of ["api-client", "tickets-endpoint", "tickets-context", "tickets-table"]) {
    await f.locator(`[data-node="${id}"]`).click();
    await page.keyboard.press("r");
  }
  await expect(progress).toHaveText("5 of 10 reviewed");
  // clicking the count does what U does: the first interaction left
  await progress.click();
  await expect(f.locator('.cm-link-label[data-edge="save"]')).toHaveAttribute("aria-pressed", "true");
  for (const id of ["save", "post", "create", "lookup", "insert"]) {
    await f.locator(`.cm-link-label[data-edge="${id}"]`).dispatchEvent("click");
    await page.keyboard.press("r");
  }
  await expect(progress).toHaveText("All 10 reviewed");
});

test("the overview's parts show a reviewed part by a check on its icon, and its comments beside its name", async ({
  page,
}) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="tickets-endpoint"]').click();
  await page.keyboard.press("r");
  await f.getByPlaceholder("Add a comment").fill("Return the ticket's id with the 409.");
  await f.getByPlaceholder("Add a comment").press("Enter");
  await page.keyboard.press("Escape");
  await f.getByRole("button", { name: "Overview" }).click();
  const row = f.getByTestId("overview").getByRole("button", { name: /^POST \/tickets/ });
  // the check sits on the icon's corner
  const icon = (await row.locator(".cm-row-icon").boundingBox())!;
  const check = (await row.locator(".cm-row-icon .cm-icon-reviewed").boundingBox())!;
  expect(check.x).toBeGreaterThan(icon.x);
  expect(check.x).toBeLessThan(icon.x + icon.width);
  expect(check.y).toBeGreaterThan(icon.y);
  await expect(row).toHaveAttribute("aria-label", /reviewed/);
  // the comments beside the name, before the status at the right
  const commented = row.locator(".cm-mark-comment");
  await expect(commented).toHaveText("1");
  const nameEnd = await row.locator(".cm-row-title").evaluate((el) => {
    const range = document.createRange();
    range.selectNodeContents(el);
    return range.getBoundingClientRect().right;
  });
  const mark = (await commented.boundingBox())!;
  expect(mark.x - nameEnd).toBeLessThan(12);
  expect(mark.x + mark.width).toBeLessThan((await row.locator(".cm-status").boundingBox())!.x);
  // a part not reviewed has no check, and no mark beside its name
  const other = f.getByTestId("overview").getByRole("button", { name: /^API client/ });
  await expect(other.locator(".cm-icon-reviewed")).toHaveCount(0);
  await expect(other.locator(".cm-mark-reviewed")).toHaveCount(0);
});

/** Opens the overview's files, a drawer at the bottom of the panel. */
const openFiles = async (f: import("@playwright/test").FrameLocator) => {
  const toggle = f.getByTestId("files").getByRole("button", { name: /^Files/ });
  if ((await toggle.getAttribute("aria-expanded")) !== "true") await toggle.click();
  return f.getByTestId("files");
};

test("the overview's files are a drawer at the bottom of the panel, which filters as you type", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 700 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  const files = f.getByTestId("files");
  const panel = f.locator(".cm-panel");
  const edge = (await panel.boundingBox())!.y + (await panel.boundingBox())!.height;
  const bottom = async () => (await files.boundingBox())!.y + (await files.boundingBox())!.height;
  // closed: a bar at the bottom, and the overview scrolls above it
  await expect(files.getByRole("button", { name: /^Files · 7/ })).toHaveAttribute("aria-expanded", "false");
  await expect(files.locator(".cm-file")).toHaveCount(0);
  expect(Math.abs((await bottom()) - edge)).toBeLessThan(12);
  await panel.evaluate((el) => el.scrollTo(0, el.scrollHeight));
  expect(Math.abs((await bottom()) - edge)).toBeLessThan(12);
  // open: the list, at the bottom, under the overview
  await openFiles(f);
  const rows = files.locator(".cm-file");
  await expect(rows).toHaveCount(7);
  expect(Math.abs((await bottom()) - edge)).toBeLessThan(12);
  await expect(rows.filter({ hasText: "lib/helpdesk/tickets.ex" })).toContainText("+19");
  // "tick" in four paths, and in the migration's name
  await files.getByPlaceholder("Filter files").fill("tick");
  await expect(rows).toHaveCount(5);
  await files.getByPlaceholder("Filter files").fill("helpdesk/tickets");
  await expect(rows).toHaveCount(2);
  // closed again, the chosen file is named on the bar
  await rows.first().click();
  await files.getByRole("button", { name: /^Files/ }).click();
  await expect(files.locator(".cm-file")).toHaveCount(0);
  await expect(files.getByRole("button", { name: /^Files/ })).toContainText("lib/helpdesk/tickets.ex");
  // not in a part's own details
  await f.locator('[data-node="api-client"]').click();
  await expect(f.getByTestId("files")).toHaveCount(0);
});

test("with a hundred files, the drawer and the overview above it scroll each on its own, under the header", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 600 });
  const review = JSON.parse(fs.readFileSync(path.join(dir, "samples", "dedup-tickets.json"), "utf8"));
  for (let i = 0; i < 100; i++)
    review.payload.files.push({ path: `lib/helpdesk/more/file_${i}.ex`, status: "added", diff: "@@ -0,0 +1 @@\n+a\n" });
  for (const entry of Object.values(review.attachments) as { path: string }[])
    entry.path = path.join(dir, "samples", entry.path);
  const file = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "cm-many-")), "review.json");
  fs.writeFileSync(file, JSON.stringify(review));
  const plugin = await mountPlugin(page, dir, { review: fixture(file) });
  const f = plugin.frame;
  const files = await openFiles(f);
  await expect(files.locator(".cm-file")).toHaveCount(107);
  const scrolls = (el: Element) => el.scrollHeight > el.clientHeight + 1 && getComputedStyle(el).overflowY !== "hidden";
  // the list scrolls inside the drawer, which stays within the panel
  const list = files.locator(".cm-finder-list");
  expect(await list.evaluate(scrolls)).toBe(true);
  const panel = (await f.locator(".cm-panel").boundingBox())!;
  const drawer = (await files.boundingBox())!;
  expect(drawer.y).toBeGreaterThan(panel.y + 40);
  expect(drawer.y + drawer.height).toBeLessThanOrEqual(panel.y + panel.height + 1);
  // the overview above it still scrolls
  const top = f.locator(".cm-panel-body");
  expect(await top.evaluate(scrolls)).toBe(true);
  await top.evaluate((el) => el.scrollTo(0, el.scrollHeight));
  // and nothing moved the view under the header
  expect((await f.locator(".cm-header").boundingBox())!.y).toBe(0);
  expect(await f.locator("body").evaluate(() => document.scrollingElement!.scrollTop)).toBe(0);
});

test("choosing a file in the overview opens it and shows on the map what touches it", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  const files = await openFiles(f);
  const file = files.locator(".cm-file", { hasText: "lib/helpdesk/tickets.ex" });
  await file.click();
  await expect(f.getByTestId("diff").locator(".cm-diff-path")).toHaveText("lib/helpdesk/tickets.ex");
  // the part and the interactions that name it, and nothing else
  await expect(f.locator(".cm-part.cm-lit")).toHaveCount(1);
  await expect(f.locator('[data-node="tickets-context"]')).toHaveClass(/cm-lit/);
  await expect(f.locator('[data-node="api-client"]')).toHaveClass(/cm-dim/);
  for (const id of ["lookup", "insert", "notify"])
    await expect(f.locator(`.cm-link-label[data-edge="${id}"]`)).toHaveClass(/cm-lit/);
  await expect(f.locator('.cm-link-label[data-edge="save"]')).toHaveClass(/cm-dim/);
  await expect(files.getByTestId("file-owners")).toHaveText(
    "On the map: Tickets, recent with key, insert and ticket created",
  );
  // choosing it again shows the whole map again
  await file.click();
  await expect(f.locator(".cm-dim")).toHaveCount(0);
  await expect(files.getByTestId("file-owners")).toHaveCount(0);
});

test("a file chosen in the overview shows in the sequence and state diagrams too", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  const files = await openFiles(f);
  await files.locator(".cm-file", { hasText: "lib/helpdesk/tickets.ex" }).click();
  await f.locator('[data-tab="first-save"]').click();
  // the messages with lines in it, or that stand for an interaction that has,
  // and the participant that stands for the part that has
  for (const id of ["m4", "m5", "m6", "m7", "m8"]) await expect(f.locator(`[data-item="${id}"]`)).toHaveClass(/cm-lit/);
  for (const id of ["m1", "m9", "m10"]) await expect(f.locator(`[data-item="${id}"]`)).toHaveClass(/cm-dim/);
  await expect(f.locator('.cm-seq-head[data-item="tickets"]')).toHaveClass(/cm-lit/);
  await expect(f.locator('.cm-seq-head[data-item="form"]')).toHaveClass(/cm-dim/);
  // another file, in the state diagram
  await files.locator(".cm-file", { hasText: "web/src/TicketForm.tsx" }).click();
  await f.locator('[data-tab="save-button"]').click();
  await expect(f.locator('.cm-part[data-item="ready"]')).toHaveClass(/cm-lit/);
  await expect(f.locator('[data-item="start"]')).toHaveClass(/cm-dim/);
  await expect(f.locator('.cm-link-label[data-item="t2"]')).toHaveClass(/cm-lit/);
});

test("the filter of the overview's files has a search icon, and a lit part no ring", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  const files = await openFiles(f);
  await expect(files.locator(".cm-finder-search svg")).toBeVisible();
  // focused, the filter has the accent border and no ring
  const filter = files.getByPlaceholder("Filter files");
  await filter.click();
  await expect(filter).toBeFocused();
  expect(await filter.evaluate((el) => getComputedStyle(el).outlineStyle)).toBe("none");
  await files.locator(".cm-file", { hasText: "lib/helpdesk/tickets.ex" }).click();
  const part = f.locator('[data-node="tickets-context"]');
  await expect(part).toHaveClass(/cm-lit/);
  expect(await part.evaluate((el) => getComputedStyle(el).boxShadow)).toBe("none");
});

test("the overview lists the files nothing on the map names, and opens one in the drawer", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  const overview = f.getByTestId("overview");
  await expect(overview.getByRole("heading", { name: "Other files" })).toBeVisible();
  const other = overview.locator(".cm-file", { hasText: "CHANGELOG.md" });
  await expect(other).toContainText("+2");
  await other.click();
  await expect(f.getByTestId("diff").locator(".cm-diff-path")).toHaveText("CHANGELOG.md");
});

test("the overview's files say how many of their lines are off the map", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: narrowed() });
  const files = await openFiles(plugin.frame);
  await expect(files.locator(".cm-file", { hasText: "TicketForm.tsx" }).locator(".cm-file-off")).toHaveText(
    "12 off map",
  );
  await expect(files.locator(".cm-file", { hasText: "api.ts" }).locator(".cm-file-off")).toHaveCount(0);
  // a changelog is no part's, and that is right
  await expect(files.locator(".cm-file", { hasText: "CHANGELOG.md" }).locator(".cm-file-off")).toHaveCount(0);
  // a code file nothing names is not on the map at all
  const review = JSON.parse(fs.readFileSync(path.join(dir, "samples", "dedup-tickets.json"), "utf8"));
  review.payload.files.push({ path: "lib/helpdesk/forgotten.ex", status: "added", diff: "@@ -0,0 +1,2 @@\n+a\n+b\n" });
  for (const entry of Object.values(review.attachments) as { path: string }[])
    entry.path = path.join(dir, "samples", entry.path);
  const extra = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "cm-files-")), "review.json");
  fs.writeFileSync(extra, JSON.stringify(review));
  const again = await mountPlugin(page, dir, { review: fixture(extra) });
  await expect(
    (await openFiles(again.frame)).locator(".cm-file", { hasText: "forgotten.ex" }).locator(".cm-file-off"),
  ).toHaveText("not on the map");
});

test("a recording sits with a part's screenshots, and plays in the drawer", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 860 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  const panel = f.getByTestId("part-detail");
  await expect(panel.getByTestId("panel-screen-position")).toHaveText("1 of 2");
  await panel.getByRole("button", { name: "Next screen" }).click();
  // in the panel: the recording's first frame, with a play mark
  const thumb = panel.locator(".cm-screen");
  await expect(thumb.locator("video")).toHaveCount(1);
  await expect(thumb.locator(".cm-screen-play")).toBeVisible();
  await thumb.click();
  // in the drawer: the video, a play button, the time and a scrubber
  const viewer = f.getByTestId("screen-viewer");
  const video = viewer.locator("video");
  await expect(video).toHaveCount(1);
  await expect(viewer.getByRole("radio", { name: "Slider" })).toHaveCount(0);
  await expect(viewer.getByTestId("video-time")).toHaveText("0:00 / 0:02");
  await viewer.getByRole("button", { name: "Play" }).click();
  await expect.poll(() => video.evaluate((v: HTMLVideoElement) => v.currentTime)).toBeGreaterThan(0.3);
  await viewer.getByRole("button", { name: "Pause" }).click();
  expect(await video.evaluate((v: HTMLVideoElement) => v.paused)).toBe(true);
  // the scrubber goes to a moment
  await viewer.getByRole("slider", { name: "Position in the recording" }).fill("2");
  await expect.poll(() => video.evaluate((v: HTMLVideoElement) => Math.round(v.currentTime))).toBe(2);
  await expect(viewer.getByTestId("video-time")).toHaveText("0:02 / 0:02");
  // the hint says what a drag on a recording does
  await expect(viewer.locator(".cm-shots-hint")).toContainText("Drag a box to comment on an area of this frame");
});

test("a comment on a recording's frame goes back with its moment, and its area when one is marked", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 860 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  const panel = f.getByTestId("part-detail");
  await panel.getByRole("button", { name: "Next screen" }).click();
  await panel.locator(".cm-screen").click();
  const viewer = f.getByTestId("screen-viewer");
  const video = viewer.locator("video");
  const scrub = viewer.getByRole("slider", { name: "Position in the recording" });
  const at = () => video.evaluate((v: HTMLVideoElement) => v.currentTime);
  await expect.poll(() => video.evaluate((v: HTMLVideoElement) => v.readyState)).toBeGreaterThan(0);

  // the whole frame, at 1 second
  await scrub.fill("1");
  await viewer.getByRole("button", { name: "Comment on this frame" }).click();
  const dialog = f.getByRole("dialog", { name: /Comment on the recording at 0:01/ });
  await dialog.getByRole("textbox").fill("The spinner starts late.");
  await dialog.getByRole("textbox").press("Enter");

  // an area of the frame, at 2 seconds: the drag pauses the video
  await scrub.fill("2");
  const box = (await video.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.55, box.y + box.height * 0.75);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.85, box.y + box.height * 0.9, { steps: 5 });
  await page.mouse.up();
  await page.keyboard.type("The label is cut off.");
  await page.keyboard.press("Enter");
  // its box shows at its moment only, and each comment marks the scrubber
  await expect(viewer.getByTestId("area")).toHaveCount(1);
  await expect(viewer.locator(".cm-scrub-mark")).toHaveCount(2);
  await scrub.fill("0");
  await expect(viewer.getByTestId("area")).toHaveCount(0);
  await viewer.locator(".cm-scrub-mark").nth(1).click();
  await expect.poll(at).toBeCloseTo(2, 1);
  await expect(viewer.getByTestId("area")).toHaveCount(1);

  // in the panel, each with its moment, which opens the recording there
  const heads = panel.getByRole("button", { name: "Show the recording at this moment" });
  await expect(heads).toHaveCount(2);
  await expect(heads.nth(0)).toContainText("Recording · Saving, recorded · 0:01");
  await scrub.fill("0");
  await heads.nth(0).click();
  await expect.poll(at).toBeCloseTo(1, 1);

  const result = (await plugin.handOver()) as {
    decision: { data: { comments: { target: Record<string, unknown> }[] } };
  };
  const [whole, area] = result.decision.data.comments.map((c) => c.target);
  expect(whole).toEqual({ screen: { node: "ticket-form", index: 1, image: "video" }, time: 1 });
  expect(area.screen).toEqual({ node: "ticket-form", index: 1, image: "video" });
  expect(area.time).toBe(2);
  expect((area.region as Record<string, number>).x).toBeCloseTo(0.55, 1);
  expect(area.image_size).toEqual({ width: 640, height: 372 });
});

test("N and P walk the parts, and Enter opens the selected part's first file", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator(".cm-part").first().waitFor();
  // the keyboard in the view
  await f.locator(".cm-header").click();
  await page.keyboard.press("n");
  await expect(f.getByTestId("part-detail")).toContainText("Ticket form");
  await page.keyboard.press("n");
  await expect(f.getByTestId("part-detail")).toContainText("API client");
  await page.keyboard.press("p");
  await page.keyboard.press("p");
  // round from the first to the last
  await expect(f.getByTestId("part-detail")).toContainText("tickets");
  await page.keyboard.press("Enter");
  await expect(f.locator(".cm-diff-path")).toHaveText("priv/repo/migrations/20261019_add_dedup_key_to_tickets.exs");
});

test("the app's forwarded keys work as the view's own: C comments, R marks reviewed", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="tickets-endpoint"]').click();
  // the app has the keyboard, and forwards the plugin's keys
  await plugin.sendKey("r");
  await expect(f.getByRole("button", { name: "Reviewed", exact: true })).toHaveAttribute("aria-pressed", "true");
  await plugin.sendKey("c");
  await expect(f.getByPlaceholder("Add a comment")).toBeFocused();
  await page.keyboard.type("Return the ticket's id with the 409.");
  await page.keyboard.press("Enter");
  await expect(f.getByTestId("part-detail")).toContainText("Return the ticket's id with the 409.");

  const result = await plugin.handOver();
  expect(result).toMatchObject({
    decision: {
      data: {
        verdict: "request_changes",
        comments: [{ target: { node: "tickets-endpoint" }, body: "Return the ticket's id with the 409." }],
        reviewed: ["tickets-endpoint"],
      },
    },
  });
});

/** The keys the view passed up to the app. */
const passedUp = async (plugin: Awaited<ReturnType<typeof mountPlugin>>) =>
  (await plugin.messages()).filter((m: { type: string }) => m.type === "key").map((m: { key?: string }) => m.key);

test("Esc closes the drawer and stays in the view; without a drawer it goes up to the app", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="tickets-context"]').click();
  await f.locator(".cm-file", { hasText: "lib/helpdesk/tickets.ex" }).click();
  await expect(f.getByTestId("diff")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(f.getByTestId("diff")).toHaveCount(0);
  await page.waitForTimeout(100);
  expect(await passedUp(plugin)).toEqual([]);

  // no drawer: the app's, which closes its side panel
  await f.locator(".cm-header").click();
  await page.keyboard.press("Escape");
  await expect.poll(() => passedUp(plugin)).toEqual(["Escape"]);
  await expect(f.getByTestId("part-detail")).toBeVisible();
});

test("Esc while a comment in the drawer is edited cancels the edit and keeps the drawer", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await f.locator(".cm-file", { hasText: "TicketForm.tsx" }).click();
  await f.locator('[data-line="new:47"]').hover();
  await f.getByRole("button", { name: "Comment on line 47" }).click();
  await f.locator("#line-comment").fill("why here?");
  await f.locator("#line-comment").press("Enter");
  const diff = f.getByTestId("diff");
  await diff.locator(".cm-comment", { hasText: "why here?" }).click();
  await diff.getByRole("textbox", { name: "Edit the comment" }).press("Escape");
  await expect(diff.getByRole("textbox", { name: "Edit the comment" })).toHaveCount(0);
  await expect(diff).toBeVisible();
  expect(await passedUp(plugin)).toEqual([]);
});

test("selecting another part of the same file scrolls the drawer to its lines", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 600 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  // the whole of tickets.ex, from its first line
  await f.locator('[data-node="tickets-context"]').click();
  await f.locator(".cm-file", { hasText: "lib/helpdesk/tickets.ex" }).click();
  const line = f.locator('[data-line="new:91"]');
  const body = f.locator(".cm-diff-body");
  const inView = async () => {
    const [l, b] = await Promise.all([line.boundingBox(), body.boundingBox()]);
    return !!l && !!b && l.y >= b.y && l.y + l.height <= b.y + b.height;
  };
  await expect.poll(inView).toBe(false);
  // the lookup claims lines 91 to 94 of the same file
  await f.locator('[data-edge="lookup"]').click();
  await expect(f.locator(".cm-diff-path")).toHaveText("lib/helpdesk/tickets.ex");
  await expect.poll(inView).toBe(true);
});

test("typing right after marking an area of a screenshot writes the comment", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 860 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await f.getByRole("button", { name: /Show Save while saving larger/ }).click();
  const after = f.getByTestId("screen-viewer").locator("figure", { hasText: "After" }).locator("img");
  const box = (await after.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.72);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.9, box.y + box.height * 0.88, { steps: 5 });
  await page.mouse.up();
  // no click in the field first: the keys go straight into it
  await page.keyboard.type("The spinner hides the label.");
  await expect(
    f.getByRole("dialog", { name: "Comment on an area of the after image" }).getByRole("textbox"),
  ).toHaveValue("The spinner hides the label.");
});

test("a drag over a screenshot comments on that area, marked on the image and listed in the sidebar", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 860 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await f.getByRole("button", { name: /Show Save while saving larger/ }).click();
  const after = f.getByTestId("screen-viewer").locator("figure", { hasText: "After" }).locator("img");
  const box = (await after.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.72);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.9, box.y + box.height * 0.88, { steps: 5 });
  await page.mouse.up();
  await page.screenshot({ path: "/tmp/cm-29.png" });
  await f
    .getByRole("dialog", { name: "Comment on an area of the after image" })
    .getByRole("textbox")
    .fill("The spinner hides the label.");
  await f.getByRole("dialog", { name: "Comment on an area of the after image" }).getByRole("textbox").press("Enter");
  await expect(f.getByTestId("screen-viewer").locator("figure", { hasText: "After" }).getByTestId("area")).toHaveCount(
    1,
  );
  // the comment is listed once, in the sidebar, not again under the screenshots
  await expect(f.getByTestId("screen-viewer")).not.toContainText("The spinner hides the label.");
  // the sidebar: the screenshot's badge, and the comment under the part's
  // comments, headed as a screenshot comment with the number of its box
  await expect(f.locator(".cm-screens-head .cm-mark")).toHaveText("1");
  const comment = f.getByTestId("part-detail").locator(".cm-comment", { hasText: "The spinner hides the label." });
  await expect(comment.getByRole("button", { name: "Show the screenshot with this area" })).toHaveText(
    "1Screenshot · Save while saving · After",
  );
  // and the part's badge on the map counts it
  await expect(f.locator('[data-node="ticket-form"] .cm-mark-comment')).toHaveText("1");
  await page.mouse.move(1100, 60);
  await page.screenshot({ path: "/tmp/cm-28.png" });

  const result = (await plugin.handOver()) as { decision: { data: { comments: { target: unknown }[] } } };
  const target = result.decision.data.comments[0].target as { screen: unknown; region: Record<string, number> };
  expect(target.screen).toEqual({ node: "ticket-form", index: 0, image: "after" });
  expect(target.region.x).toBeCloseTo(0.6, 1);
  expect(target.region.width).toBeCloseTo(0.3, 1);
  // the same area in the image's own pixels, for an agent that crops the file
  const sized = target as unknown as { pixels: Record<string, number>; image_size: Record<string, number> };
  expect(sized.image_size).toEqual({ width: 1032, height: 600 });
  expect(sized.pixels.x).toBe(Math.round(target.region.x * 1032));
  expect(sized.pixels.y).toBe(Math.round(target.region.y * 600));
  expect(sized.pixels.width).toBe(Math.round(target.region.width * 1032));
  expect(sized.pixels.height).toBe(Math.round(target.region.height * 600));
});

test("the wheel zooms a screenshot on the pointer, keeping that spot under it", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 860 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await f.getByRole("button", { name: /Show Save while saving larger/ }).click();
  const viewer = f.getByTestId("screen-viewer");
  await expect(viewer.getByTestId("zoom-level")).toHaveText("Fit");
  const img = viewer.locator("figure", { hasText: "After" }).locator("img");
  // the share of the after image under a point of the page
  const under = async (x: number, y: number) => {
    const b = (await img.boundingBox())!;
    return [Math.round(((x - b.x) / b.width) * 100), Math.round(((y - b.y) / b.height) * 100)];
  };
  const i = (await img.boundingBox())!;
  const x = i.x + i.width * 0.8;
  const y = i.y + i.height * 0.75;
  const before = await under(x, y);
  await page.mouse.move(x, y);
  for (let n = 0; n < 5; n++) await page.mouse.wheel(0, -120);
  await expect(viewer.getByTestId("zoom-level")).not.toHaveText("Fit");
  expect((await img.boundingBox())!.width).toBeGreaterThan(i.width * 1.5);
  const after = await under(x, y);
  expect(Math.abs(after[0] - before[0])).toBeLessThanOrEqual(1);
  expect(Math.abs(after[1] - before[1])).toBeLessThanOrEqual(1);
  // the before follows the same view
  const pair = viewer.locator("figure", { hasText: "Before" }).locator("img");
  expect(Math.round((await pair.boundingBox())!.width)).toBe(Math.round((await img.boundingBox())!.width));

  // the buttons zoom on the middle, and Fit shows the whole image again
  await viewer.getByRole("button", { name: "Zoom out" }).click();
  await viewer.getByRole("button", { name: "Fit" }).click();
  await expect(viewer.getByTestId("zoom-level")).toHaveText("Fit");
  expect(Math.round((await img.boundingBox())!.width)).toBe(Math.round(i.width));
});

test("an area stays on its spot as the screenshot zooms, and Space with a drag pans", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 860 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await f.getByRole("button", { name: /Show Save while saving larger/ }).click();
  const viewer = f.getByTestId("screen-viewer");
  const figure = viewer.locator("figure", { hasText: "After" });
  const img = figure.locator("img");
  const b = (await img.boundingBox())!;
  await page.mouse.click(b.x + b.width * 0.5, b.y + b.height * 0.5);
  await f.getByRole("dialog").getByRole("textbox").fill("here");
  await f.getByRole("dialog").getByRole("textbox").press("Enter");
  const share = async () => {
    const [i, a] = await Promise.all([img.boundingBox(), figure.getByTestId("area").boundingBox()]);
    return Math.round(((a!.x + a!.width / 2 - i!.x) / i!.width) * 100);
  };
  expect(await share()).toBe(50);
  await viewer.getByRole("button", { name: "Zoom in" }).click();
  await viewer.getByRole("button", { name: "Zoom in" }).click();
  expect(await share()).toBe(50);

  // Space held: a drag pans, both images together
  const start = (await img.boundingBox())!;
  const pairStart = (await viewer.locator("figure", { hasText: "Before" }).locator("img").boundingBox())!;
  const stage = (await figure.getByTestId("stage").boundingBox())!;
  await page.mouse.move(stage.x + 100, stage.y + 100);
  await page.keyboard.down(" ");
  await page.mouse.down();
  await page.mouse.move(stage.x + 130, stage.y + 115, { steps: 3 });
  // the images follow the pointer while the drag lasts
  await expect.poll(async () => Math.round((await img.boundingBox())!.x - start.x)).toBe(30);
  await page.mouse.move(stage.x + 160, stage.y + 130, { steps: 3 });
  await page.mouse.up();
  await page.keyboard.up(" ");
  const moved = (await img.boundingBox())!;
  expect(Math.round(moved.x - start.x)).toBe(60);
  expect(Math.round(moved.y - start.y)).toBe(30);
  const pairMoved = (await viewer.locator("figure", { hasText: "Before" }).locator("img").boundingBox())!;
  expect(Math.round(pairMoved.x - pairStart.x)).toBe(60);
  // panning marks nothing
  await expect(f.getByRole("dialog")).toHaveCount(0);
});

test("the overview shows the comments on each part, its lines included", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="tickets-endpoint"]').click();
  await f.getByPlaceholder("Add a comment").fill("Return the id.");
  await f.getByPlaceholder("Add a comment").press("Enter");
  await f.locator('[data-node="ticket-form"]').click();
  await f.locator(".cm-file", { hasText: "TicketForm.tsx" }).click();
  await f.locator('[data-line="new:47"]').hover();
  await f.getByRole("button", { name: "Comment on line 47" }).click();
  await f.locator("#line-comment").fill("why here?");
  await f.locator("#line-comment").press("Enter");
  await f.getByRole("button", { name: "Close the diff" }).click();
  await f.locator(".cm-header").click();
  await page.keyboard.press("Escape");
  // back to the overview
  await f.locator(".cm-back").click();
  await f.locator(".cm-back").click();
  const overview = f.getByTestId("overview");
  await expect(overview.locator(".cm-row", { hasText: "POST /tickets" }).locator(".cm-mark")).toHaveText("1");
  await expect(overview.locator(".cm-row", { hasText: "Ticket form" }).locator(".cm-mark")).toHaveText("1");
  await expect(overview.locator(".cm-row", { hasText: "API client" }).locator(".cm-mark")).toHaveCount(0);
});

test("a held Space keeps the hand, through the release and press pairs a held key may send", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await f.getByRole("button", { name: /Show Save while saving larger/ }).click();
  const stage = f.getByTestId("screen-viewer").locator("figure", { hasText: "After" }).getByTestId("stage");
  // the key events and the pan class as the view sees them, sampled after each
  const seen = await stage.evaluate(async (el) => {
    const send = (type: string, repeat = false) =>
      window.dispatchEvent(new KeyboardEvent(type, { key: " ", code: "Space", repeat, bubbles: true }));
    const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
    const out: boolean[] = [];
    send("keydown");
    await wait(30);
    out.push(el.classList.contains("cm-stage-pan"));
    for (let n = 0; n < 5; n++) {
      send("keyup");
      await wait(5);
      out.push(el.classList.contains("cm-stage-pan"));
      send("keydown", true);
      await wait(25);
      out.push(el.classList.contains("cm-stage-pan"));
    }
    send("keyup");
    await wait(150);
    out.push(el.classList.contains("cm-stage-pan"));
    return out;
  });
  expect(seen.slice(0, -1).every(Boolean)).toBe(true);
  // a real release ends it
  expect(seen[seen.length - 1]).toBe(false);
});

test("no part hides a label, even of a long line that passes another part", async ({ page }) => {
  const payload = {
    nodes: [
      { id: "a", title: "Form" },
      { id: "b", title: "Client" },
      { id: "c", title: "Endpoint" },
    ],
    edges: [
      { id: "ab", from: "a", to: "b", label: "submit" },
      { id: "bc", from: "b", to: "c", label: "POST /tickets" },
      { id: "ac", from: "a", to: "c", label: "fetch(execution_status, job_id)" },
    ],
    files: [{ path: "a.ts", diff: "@@ -1 +1 @@\n-a\n+b\n" }],
  };
  const plugin = await mountPlugin(page, dir, { review: { title: "labels", payload } });
  const f = plugin.frame;
  await expect(f.locator(".cm-link-label")).toHaveCount(3);
  const hidden = await f.locator("body").evaluate(() => {
    const parts = [...document.querySelectorAll(".cm-part")].map((p) => p.getBoundingClientRect());
    return [...document.querySelectorAll(".cm-link-label")]
      .filter((l) => {
        const r = l.getBoundingClientRect();
        return parts.some((p) => r.left < p.right && r.right > p.left && r.top < p.bottom && r.bottom > p.top);
      })
      .map((l) => l.textContent);
  });
  expect(hidden).toEqual([]);
});

test("a selected part's ring stays clear of its lane's title", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  for (const id of ["ticket-form", "tickets-endpoint", "tickets-table"]) {
    await f.locator(`[data-node="${id}"]`).click();
    const gap = await f.locator(`[data-node="${id}"]`).evaluate((part) => {
      const node = part.closest(".react-flow__node")!;
      const lanes = [...document.querySelectorAll(".cm-lane")];
      const p = part.getBoundingClientRect();
      // the lane the part sits in, and its title
      const lane = lanes.find((l) => {
        const r = l.getBoundingClientRect();
        return p.left >= r.left && p.right <= r.right && p.top >= r.top && p.bottom <= r.bottom;
      });
      const title = lane!.querySelector(".cm-lane-title")!.getBoundingClientRect();
      // the ring reaches 4 pixels past the card, at the diagram's scale
      const scale = p.width / (part as HTMLElement).offsetWidth;
      return Math.round(p.top - 4 * scale - title.bottom) + (node ? 0 : 0);
    });
    expect(gap, id).toBeGreaterThanOrEqual(2);
  }
});

test("a diff with git's header lines shows only its hunks, numbered from the hunk", async ({ page }) => {
  const diff = [
    "diff --git a/lib/monitor.ex b/lib/monitor.ex",
    "new file mode 100644",
    "index 0000000000..1e8fa77031",
    "--- /dev/null",
    "+++ b/lib/monitor.ex",
    "@@ -0,0 +1,3 @@",
    "+defmodule Monitor do",
    "+  -- not a header, though it starts with dashes",
    "+end",
  ].join("\n");
  const payload = {
    nodes: [{ id: "m", title: "Monitor", changes: [{ file: "lib/monitor.ex" }] }],
    files: [{ path: "lib/monitor.ex", status: "added", diff }],
  };
  const plugin = await mountPlugin(page, dir, { review: { title: "header", payload } });
  const f = plugin.frame;
  await f.locator('[data-node="m"]').click();
  await expect(f.locator(".cm-files .cm-counts")).toHaveText("+3 −0");
  await f.locator(".cm-files .cm-row").first().click();
  const rows = f.locator("tr.cm-line:not(.cm-line-hunk)");
  await expect(rows).toHaveCount(3);
  await expect(rows.first()).toHaveAttribute("data-line", "new:1");
  await expect(rows.first()).toContainText("defmodule Monitor do");
  await expect(f.locator(".cm-diff")).not.toContainText("file mode");
});

test("the diff drawer has the text cursor and lets the person select its code", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await page.keyboard.press("Enter");
  const code = f.locator('[data-line="new:47"] td').last();
  await code.waitFor();
  const style = await code.evaluate((el) => ({
    cursor: getComputedStyle(el).cursor,
    select: getComputedStyle(el).userSelect,
  }));
  expect(style.cursor).not.toBe("crosshair");
  expect(style.select).not.toBe("none");
});

test("a long file path ends in an ellipsis and stays inside the panel", async ({ page }) => {
  const file = "apps/jobs_ui/lib/jobex/execution_monitor/crashed_executions.ex";
  const payload = {
    nodes: [{ id: "m", title: "Monitor", changes: [{ file, lines: [[12, 30]] }] }],
    files: [{ path: file, diff: "@@ -12,1 +12,1 @@\n-a\n+b\n" }],
  };
  const plugin = await mountPlugin(page, dir, { review: { title: "path", payload } });
  const f = plugin.frame;
  await f.locator('[data-node="m"]').click();
  await expect(f.locator(".cm-files .cm-row")).toBeVisible();
  const panel = await f.locator(".cm-panel").evaluate((p) => ({ scroll: p.scrollWidth, width: p.clientWidth }));
  expect(panel.scroll).toBeLessThanOrEqual(panel.width);
});

test("an entity of an extra diagram takes comments and the reviewed mark under its diagram", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-tab="save-button"]').click();
  await f.locator('[data-item="t4"]').click();
  const detail = f.getByTestId("item-detail");
  await expect(detail).toContainText("failed / setSaving(false)");
  await expect(detail).toContainText("State diagram: The Save button");
  await plugin.sendKey("r");
  await plugin.sendKey("c");
  await expect(f.getByPlaceholder("Add a comment")).toBeFocused();
  await page.keyboard.type("Show why it failed.");
  await page.keyboard.press("Enter");
  // the badges on the transition and on the tab
  await expect(f.locator('[data-item="t4"] .cm-mark-comment')).toHaveText("1");
  await expect(f.locator('[data-tab="save-button"] .cm-tab-count')).toHaveText("1");
  // its file opens in the drawer at its lines, with the diagram focused
  // rather than the comment field
  await f.locator('[data-item="t4"]').click();
  await plugin.sendKey("Enter");
  await expect(f.locator(".cm-diff-path")).toHaveText("web/src/TicketForm.tsx");

  const result = await plugin.handOver();
  expect(result).toMatchObject({
    decision: {
      data: {
        comments: [
          {
            target: { diagram: "save-button", item: "t4" },
            body: "Show why it failed.",
          },
        ],
        reviewed: ["save-button:t4"],
      },
    },
  });
});

test("a message that names a map interaction selects it, and N walks the messages in order", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.getByRole("button", { name: "Save pressed twice" }).click();
  await expect(f.locator('[data-tab="second-save"]')).toHaveAttribute("aria-selected", "true");
  // n2 is the map's POST /tickets interaction
  await f.locator('[data-item="n2"]').click();
  await expect(f.getByTestId("link-detail")).toContainText("POST /tickets");
  await expect(f.getByTestId("link-detail")).toContainText("Also in");
  await expect(f.locator('[data-item="n2"]')).toHaveAttribute("aria-pressed", "true");
  await plugin.sendKey("n");
  await expect(f.getByTestId("link-detail")).toContainText("create/2");
  await plugin.sendKey("n");
  await plugin.sendKey("n");
  // n5 has no interaction of its own on the map
  await expect(f.getByTestId("item-detail")).toContainText("the ticket");
  // and the Back link returns to the interaction before
  await f.locator(".cm-back").click();
  await expect(f.getByTestId("link-detail")).toContainText("recent with key");
});

test("After only shows the latest image alone, takes area comments, and is remembered", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 860 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await f.getByRole("button", { name: /Show Save while saving larger/ }).click();
  const viewer = f.getByTestId("screen-viewer");
  await expect(viewer.locator("figure")).toHaveCount(2);
  await viewer.getByRole("radio", { name: "After only" }).click();
  await expect(viewer.locator("figure")).toHaveCount(1);
  await expect(viewer.locator("figure figcaption")).toHaveText("After");
  // the choice is a setting, which the app keeps for the next review
  await expect.poll(() => plugin.lastSettingsSet()).toMatchObject({ screen_mode: "after" });
  // the image takes the whole width the two shared
  const img = viewer.locator("figure img");
  const box = (await img.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.2, box.y + box.height * 0.2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.5, { steps: 5 });
  await page.mouse.up();
  const dialog = f.getByRole("dialog", { name: "Comment on an area of the after image" });
  await dialog.getByRole("textbox").fill("Align the fields.");
  await dialog.getByRole("textbox").press("Enter");
  await expect(viewer.getByTestId("area")).toHaveCount(1);
  // closed and opened again, it shows the after alone
  await page.keyboard.press("Escape");
  await f.getByRole("button", { name: /Show Save while saving larger/ }).click();
  await expect(viewer.locator("figure")).toHaveCount(1);
  await expect(viewer.getByRole("radio", { name: "After only" })).toHaveAttribute("aria-checked", "true");
});

test("a review opens its screenshots as the screen_mode setting says", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup(), settings: { screen_mode: "after" } });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await f.getByRole("button", { name: /Show Save while saving larger/ }).click();
  await expect(f.getByTestId("screen-viewer").locator("figure")).toHaveCount(1);
});

test("with the drawer showing, the diagram fits above it, also after a resize", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  const plugin = await mountPlugin(page, dir, { review: dedup(), settings: { show_diff: true } });
  const f = plugin.frame;
  const drawer = f.locator(".cm-diff");
  await drawer.waitFor();
  /** the parts that reach under the drawer's top edge */
  const hidden = () =>
    f.locator("body").evaluate(() => {
      const top = document.querySelector(".cm-diff")!.getBoundingClientRect().top;
      return [...document.querySelectorAll(".cm-part")]
        .filter((p) => p.getBoundingClientRect().bottom > top)
        .map((p) => p.textContent);
    });
  await expect.poll(hidden).toEqual([]);
  // drag the drawer's top edge up
  const edge = (await f.locator(".cm-diff-resize").boundingBox())!;
  await page.mouse.move(edge.x + edge.width / 2, edge.y + edge.height / 2);
  await page.mouse.down();
  await page.mouse.move(edge.x + edge.width / 2, edge.y - 150, { steps: 5 });
  await page.mouse.up();
  await expect.poll(hidden).toEqual([]);
});

test("a diagram's tab counts the comments on its linked participants and messages", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-tab="second-save"]').click();
  // n2 is the map's POST /tickets interaction
  await f.locator('[data-item="n2"]').click();
  await plugin.sendKey("c");
  await expect(f.getByPlaceholder("Add a comment")).toBeFocused();
  await page.keyboard.type("Send the key on the retry too.");
  await page.keyboard.press("Enter");
  await expect(f.locator('[data-item="n2"] .cm-mark-comment')).toHaveText("1");
  await expect(f.locator('[data-tab="second-save"] .cm-tab-count')).toHaveText("1");
  // the first save links the same interaction
  await expect(f.locator('[data-tab="first-save"] .cm-tab-count')).toHaveText("1");
  // a map part's comment reaches the diagrams it takes part in, once each
  await f.locator('[data-item="form"]').click();
  await plugin.sendKey("c");
  await expect(f.getByPlaceholder("Add a comment")).toBeFocused();
  await page.keyboard.type("Disable the whole form while saving.");
  await page.keyboard.press("Enter");
  await expect(f.locator('[data-tab="second-save"] .cm-tab-count')).toHaveText("2");
  await expect(f.locator('[data-tab="save-button"] .cm-tab-count')).toHaveCount(0);
  // and the overview's list of diagrams, with nothing selected
  await f.locator('[data-tab="map"]').click();
  await f.locator(".react-flow__pane").click({ position: { x: 10, y: 10 } });
  await expect(f.locator('[data-diagram-row="second-save"] .cm-mark-comment')).toHaveText("2");
});

test("a line comment badges the one entity that holds the line most narrowly", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await f.locator(".cm-file", { hasText: "TicketForm.tsx" }).click();
  await f.locator('[data-line="new:49"]').hover();
  await f.getByRole("button", { name: "Comment on line 49" }).click();
  await f.locator("#line-comment").fill("Await the save here?");
  await f.locator("#line-comment").press("Enter");
  // the map: createTicket holds lines 47-49, the form 40-68
  await expect(f.locator('[data-edge="save"] .cm-mark-comment')).toHaveText("1");
  await expect(f.locator('[data-node="ticket-form"] .cm-mark-comment')).toHaveCount(0);
  // the first save: the ticket reply holds 49-50, the createTicket message 47-49
  await f.locator('[data-tab="first-save"]').click();
  await expect(f.locator(".cm-diagram .cm-mark-comment")).toHaveCount(1);
  await expect(f.locator('[data-item="m10"] .cm-mark-comment')).toHaveText("1");
  await expect(f.locator('[data-tab="first-save"] .cm-tab-count')).toHaveText("1");
});

test("a comment on a range of lines badges the entity that holds the most of it, a dot included", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-tab="save-button"]').click();
  await f.locator('[data-item="t4"]').click();
  await page.keyboard.press("Enter");
  // drag over lines 49 to 51
  await f.locator('[data-line="new:49"]').hover();
  const from = (await f.getByRole("button", { name: "Comment on line 49" }).boundingBox())!;
  const to = (await f.locator('[data-line="new:51"]').boundingBox())!;
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x + from.width / 2, to.y + to.height / 2, { steps: 5 });
  await page.mouse.up();
  await f.locator("#line-comment").fill("Handle a failed save.");
  await f.locator("#line-comment").press("Enter");
  // saved / onSaved holds 49-50, two of the lines; failed / setSaving 51-53
  // and the end state 50 hold one each
  await expect(f.locator('.cm-diagram [data-item="t5"] .cm-mark-comment')).toHaveText("1");
  await expect(f.locator(".cm-diagram .cm-mark-comment")).toHaveCount(1);
  await expect(f.locator('[data-tab="save-button"] .cm-tab-count')).toHaveText("1");
  // a comment on the end state shows on its dot
  await f.locator('[data-item="saved"]').click();
  await f.getByPlaceholder("Add a comment").fill("Close the form here.");
  await f.getByPlaceholder("Add a comment").press("Enter");
  await expect(f.locator('.cm-diagram [data-item="saved"] .cm-mark-comment')).toHaveText("1");
  await expect(f.locator('[data-tab="save-button"] .cm-tab-count')).toHaveText("2");
});

test("a sequence participant's head is wide enough for its title", async ({ page }) => {
  const payload = {
    nodes: [{ id: "a", title: "A" }],
    diagrams: [
      {
        id: "s",
        kind: "sequence",
        title: "Names",
        participants: [
          { id: "p1", title: "SequenceDiagram" },
          { id: "p2", title: "ExecutionMonitorSupervisor" },
          { id: "p3", title: "Panel" },
        ],
        messages: [{ id: "m1", from: "p1", to: "p2", label: "call" }],
      },
    ],
    files: [{ path: "a.ts", diff: "@@ -1 +1 @@\n-a\n+b\n" }],
  };
  const plugin = await mountPlugin(page, dir, { review: { title: "names", payload } });
  const f = plugin.frame;
  // a wider system font than the one the test runs with, as on another
  // platform: the heads must fit the titles as drawn
  await f
    .locator("body")
    .evaluate(() => document.documentElement.style.setProperty("--pinrail-sans", "Verdana, sans-serif"));
  await f.locator('[data-tab="s"]').click();
  await f.locator('[data-item="p1"]').waitFor();
  const cut = await f
    .locator("body")
    .evaluate(() =>
      [...document.querySelectorAll(".cm-seq-head .cm-part-title")]
        .filter((t) => t.scrollWidth > t.clientWidth)
        .map((t) => t.textContent),
    );
  expect(cut).toEqual([]);
});

test("the panel shows a part's screens one at a time, and steps the viewer with them", async ({ page }) => {
  const review = dedup();
  const form = review.payload.nodes.find((n: { id: string }) => n.id === "ticket-form");
  form.screens = [form.screens[0], { ...form.screens[0], title: "Dark theme" }];
  const plugin = await mountPlugin(page, dir, { review });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  const panel = f.getByTestId("part-detail");
  await expect(panel.locator(".cm-screen")).toHaveCount(1);
  await expect(panel.getByTestId("panel-screen-position")).toHaveText("1 of 2");
  await panel.getByRole("button", { name: "Next screen" }).click();
  await expect(panel.locator(".cm-screens-title")).toHaveText("Dark theme");
  // the viewer opens on that screen, and the panel follows the viewer back
  await panel.locator(".cm-screen").click();
  await expect(f.getByTestId("screen-position")).toHaveText("2 of 2");
  await f.getByTestId("screen-viewer").getByRole("button", { name: "Previous screen" }).click();
  await expect(panel.getByTestId("panel-screen-position")).toHaveText("1 of 2");
  // and stepping in the panel steps the viewer
  await panel.getByRole("button", { name: "Next screen" }).click();
  await expect(f.getByTestId("screen-position")).toHaveText("2 of 2");
});

test("the screen viewer's header has room for the screen's name", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 760 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await f.getByRole("button", { name: /Show Save while saving larger/ }).click();
  const name = f.locator(".cm-shots-name");
  await expect(name).toHaveText("Save while saving");
  const fits = await name.evaluate((el) => el.scrollWidth <= el.clientWidth);
  expect(fits).toBe(true);
  // the hint is still there, under the images
  await expect(f.locator(".cm-shots-hint")).toContainText("scroll to zoom");
});

test("a file row keeps room for the path, with many line ranges", async ({ page }) => {
  const file = "plugins/change-map/src/App.tsx";
  const payload = {
    nodes: [
      {
        id: "app",
        title: "App",
        changes: [
          {
            file,
            lines: [
              [71, 73],
              [206, 250],
              [395, 425],
            ],
          },
        ],
      },
    ],
    files: [{ path: file, diff: "@@ -71,1 +71,1 @@\n-a\n+b\n" }],
  };
  const plugin = await mountPlugin(page, dir, { review: { title: "ranges", payload } });
  const f = plugin.frame;
  await f.locator('[data-node="app"]').click();
  const path = f.locator(".cm-file .cm-row-title");
  await expect(path).toHaveText(file);
  expect(await path.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
  await expect(f.locator(".cm-file")).toContainText("71–73, 206–250, 395–425");
});

test("the microphone in the comment field shows that it is recording, and its words can be added", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup(), dictation: true });
  const f = plugin.frame;
  await f.locator('[data-node="tickets-endpoint"]').click();
  const mic = f.locator(".pinrail-compose .pinrail-dictate");
  await expect(mic).toBeVisible();
  // the microphone lines up with the add button
  const middle = async (sel: string) => {
    const b = (await f.locator(sel).boundingBox())!;
    return b.y + b.height / 2;
  };
  expect(
    Math.abs((await middle(".pinrail-compose .pinrail-dictate")) - (await middle(".pinrail-compose-send"))),
  ).toBeLessThan(1);
  const idle = await mic.evaluate((el) => getComputedStyle(el).color);
  await mic.click();
  await expect(mic).toHaveAttribute("aria-pressed", "true");
  // the SDK draws a recording microphone in the danger colour
  await expect.poll(() => mic.evaluate((el) => getComputedStyle(el).color)).not.toBe(idle);
  await plugin.say("Return the id with the 409.");
  await plugin.endDictation();
  await expect(mic).not.toHaveAttribute("aria-pressed", "true");
  // away from the microphone, which is lighter under the pointer
  await page.mouse.move(0, 0);
  await expect.poll(() => mic.evaluate((el) => getComputedStyle(el).color)).toBe(idle);
  await f.getByRole("button", { name: "Add comment" }).click();
  await expect(f.getByTestId("part-detail")).toContainText("Return the id with the 409.");
});

test("the comment field grows with its text, and gives the buttons a row once the text wraps", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="tickets-endpoint"]').click();
  const box = f.locator(".pinrail-compose");
  const field = f.getByPlaceholder("Add a comment");
  const scrolls = () => field.evaluate((el) => el.scrollHeight > el.clientHeight + 1);
  await field.fill("Return the id.");
  await expect(box).not.toHaveClass(/pinrail-compose-wide/);
  const one = (await field.boundingBox())!.height;
  // text that reaches the buttons
  await field.fill("Return the id of the ticket the first request created, with the 409 and its status.");
  await expect(box).toHaveClass(/pinrail-compose-wide/);
  await expect(box.locator(".pinrail-compose-keys")).toHaveText("Enter to add · Shift+Enter for a new line");
  expect(await scrolls()).toBe(false);
  // four lines: taller, and no scrollbar
  await field.fill("One\nTwo\nThree\nFour");
  const four = (await field.boundingBox())!.height;
  expect(four).toBeGreaterThan(one * 3);
  expect(await scrolls()).toBe(false);
  // past the limit it stops growing and scrolls
  await field.fill(Array.from({ length: 20 }, (_, i) => `Line ${i + 1}`).join("\n"));
  expect((await field.boundingBox())!.height).toBeLessThanOrEqual(150);
  expect(await scrolls()).toBe(true);
  // empty again, it is one line
  await field.fill("");
  await expect(box).not.toHaveClass(/pinrail-compose-wide/);
  await expect(box.locator(".pinrail-compose-keys")).toBeHidden();
});

test("the area comment field stays inside its dialog when it takes a second line", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 860 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await f.getByRole("button", { name: /Show Save while saving larger/ }).click();
  const img = f.getByTestId("screen-viewer").locator("figure", { hasText: "Before" }).locator("img");
  const b = (await img.boundingBox())!;
  await page.mouse.move(b.x + b.width * 0.3, b.y + b.height * 0.2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width * 0.5, b.y + b.height * 0.35, { steps: 4 });
  await page.mouse.up();
  const field = f.locator("#area-comment");
  await field.pressSequentially("A");
  await field.press("Shift+Enter");
  await field.pressSequentially("very large");
  await expect(f.locator(".cm-pop .pinrail-compose")).toHaveClass(/pinrail-compose-wide/);
  const pop = (await f.locator(".cm-pop").boundingBox())!;
  const box = (await f.locator(".cm-pop .pinrail-compose").boundingBox())!;
  expect(box.x + box.width).toBeLessThanOrEqual(pop.x + pop.width);
  // wide enough for the keys on the buttons' row
  const keys = f.locator(".cm-pop .pinrail-compose-keys");
  expect(await keys.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
});

test("the hand-over says Approve with comments when the person approves and has commented", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await expect.poll(() => plugin.lastStatus()).toBe("Approve");
  await f.locator('[data-node="tickets-endpoint"]').click();
  await f.getByPlaceholder("Add a comment").fill("Return the id.");
  await f.getByPlaceholder("Add a comment").press("Enter");
  // a comment asks for changes until the person chooses
  await expect.poll(() => plugin.lastStatus()).toBe("Request changes");
  await f.getByRole("radio", { name: "Approve" }).click();
  await expect.poll(() => plugin.lastStatus()).toBe("Approve with comments");
});

test("a comment has edit and delete at its top right, shown on hover", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="tickets-endpoint"]').click();
  await f.getByPlaceholder("Add a comment").fill("Return the id.");
  await f.getByPlaceholder("Add a comment").press("Enter");
  const comment = f.locator(".cm-comment", { hasText: "Return the id." });
  await expect(comment).toHaveClass(/pinrail-comment/);
  await comment.hover();
  await comment.getByRole("button", { name: "Edit the comment" }).click();
  await expect(f.getByRole("textbox", { name: "Edit the comment" })).toBeFocused();
  await f.getByRole("textbox", { name: "Edit the comment" }).press("Escape");
  await comment.hover();
  await comment.getByRole("button", { name: "Delete the comment" }).click();
  await expect(f.locator(".cm-comment", { hasText: "Return the id." })).toHaveCount(0);
});

test("editing a comment uses the comment field, with the cursor at the end", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="tickets-endpoint"]').click();
  await f.getByPlaceholder("Add a comment").fill("Return the id.");
  await f.getByPlaceholder("Add a comment").press("Enter");
  await f.locator(".cm-comment-editable", { hasText: "Return the id." }).click();
  const edit = f.getByRole("textbox", { name: "Edit the comment" });
  await expect(edit).toBeFocused();
  await expect(f.locator(".cm-comment-editing .pinrail-compose")).toBeVisible();
  expect(await edit.evaluate((el: HTMLTextAreaElement) => [el.selectionStart, el.value.length])).toEqual([14, 14]);
  expect(await edit.evaluate((el) => getComputedStyle(el).resize)).toBe("none");
  // typing goes after the text, and the cancel button keeps the comment as it was
  await page.keyboard.type(" And its status.");
  await f.locator(".cm-comment-editing").getByRole("button", { name: "Cancel" }).click();
  await expect(f.getByTestId("part-detail")).toContainText("Return the id.");
  await expect(f.getByTestId("part-detail")).not.toContainText("And its status.");
  // Enter saves the edit
  await f.locator(".cm-comment-editable", { hasText: "Return the id." }).click();
  await page.keyboard.type(" And its status.");
  await page.keyboard.press("Enter");
  await expect(f.getByTestId("part-detail")).toContainText("Return the id. And its status.");
});

test("the side panel resizes from its left edge, no narrower than it starts, and keeps its width", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 760 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  const panel = f.locator(".cm-panel");
  await expect.poll(async () => (await panel.boundingBox())!.width).toBe(340);
  const edge = f.locator(".cm-panel-resize");
  const drag = async (dx: number) => {
    const e = (await edge.boundingBox())!;
    await page.mouse.move(e.x + e.width / 2, e.y + 200);
    await page.mouse.down();
    await page.mouse.move(e.x + e.width / 2 + dx, e.y + 200, { steps: 5 });
    await page.mouse.up();
  };
  // not narrower than it starts
  await drag(120);
  expect((await panel.boundingBox())!.width).toBe(340);
  // wider to the left, and the width is kept as a setting
  await drag(-200);
  expect((await panel.boundingBox())!.width).toBe(540);
  await expect.poll(() => plugin.lastSettingsSet()).toMatchObject({ panel_width: 540 });
});

test("a review opens the side panel at the panel_width setting", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 760 });
  const plugin = await mountPlugin(page, dir, { review: dedup(), settings: { panel_width: 500 } });
  await expect.poll(async () => (await plugin.frame.locator(".cm-panel").boundingBox())!.width).toBe(500);
});

test("the comment field stays at the bottom of the side panel while its details scroll", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 520 });
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  const field = f.locator(".cm-panel-foot").getByPlaceholder("Add a comment");
  await expect(field).toBeVisible();
  const panel = (await f.locator(".cm-panel").boundingBox())!;
  const foot = (await f.locator(".cm-panel-foot").boundingBox())!;
  expect(Math.abs(foot.y + foot.height - (panel.y + panel.height))).toBeLessThan(1);
  // the whole panel scrolls, under the field, which stays where it is
  const scroll = f.locator(".cm-panel");
  await expect(scroll.locator(".cm-panel-foot")).toHaveCount(1);
  expect(await scroll.evaluate((el) => getComputedStyle(el).overscrollBehaviorY)).toBe("none");
  expect(await scroll.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
  await scroll.evaluate((el) => el.scrollTo(0, el.scrollHeight));
  expect((await f.locator(".cm-panel-foot").boundingBox())!.y).toBe(foot.y);
  // a click reaches it through the fade over the details
  await field.click();
  await expect(field).toBeFocused();
  await f.locator(".cm-header").click();
  // C still focuses it
  await plugin.sendKey("c");
  await expect(field).toBeFocused();
  // the overview has no field: its footer holds the change's files
  await f.locator(".cm-back").click();
  await expect(f.getByTestId("overview")).toBeVisible();
  await expect(f.locator(".cm-panel-foot").getByPlaceholder("Add a comment")).toHaveCount(0);
  await expect(f.locator(".cm-panel-foot").getByTestId("files")).toBeVisible();
});

test("the slider fits its images in the drawer, with the handle on the split", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  const plugin = await mountPlugin(page, dir, { review: dedup(), settings: { screen_mode: "wipe" } });
  const f = plugin.frame;
  await f.locator('[data-node="ticket-form"]').click();
  await f.getByRole("button", { name: /Show Save while saving larger/ }).click();
  const wipe = f.getByTestId("wipe");
  const after = wipe.getByRole("img", { name: /after/ });
  await expect(after).toBeVisible();
  await expect.poll(() => after.evaluate((img: HTMLImageElement) => img.complete)).toBe(true);
  const body = (await f.locator(".cm-shots-body").boundingBox())!;
  const image = (await after.boundingBox())!;
  // the whole image in view, not its corner
  expect(image.width).toBeLessThanOrEqual(body.width + 1);
  expect(image.y + image.height).toBeLessThanOrEqual(body.y + body.height + 1);
  // the handle at the middle of the image, where the before is cut
  const handle = (await wipe.getByRole("slider").boundingBox())!;
  expect(Math.abs(handle.x + handle.width / 2 - (image.x + image.width / 2))).toBeLessThan(2);
});

test("the zoom buttons sit in the map's top-left corner, above the diff drawer", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 760 });
  const plugin = await mountPlugin(page, dir, { review: dedup(), settings: { show_diff: true } });
  const f = plugin.frame;
  await f.locator(".cm-diff").waitFor();
  const fit = f.getByRole("button", { name: "Fit to the frame" });
  const drawer = (await f.locator(".cm-diff").boundingBox())!;
  const button = (await fit.boundingBox())!;
  expect(button.y + button.height).toBeLessThanOrEqual(drawer.y);
  const map = (await f.locator(".cm-diagram").boundingBox())!;
  const bar = (await f.locator(".cm-zoom").boundingBox())!;
  expect(bar.x - map.x).toBeLessThan(20);
  expect(bar.y - map.y).toBeLessThan(20);
  await fit.click({ timeout: 2000 });
});

test("a comment typed on one part is not carried to the next part selected", async ({ page }) => {
  const plugin = await mountPlugin(page, dir, { review: dedup() });
  const f = plugin.frame;
  await f.locator('[data-node="tickets-endpoint"]').click();
  await f.getByPlaceholder("Add a comment").fill("rename this");
  await f.locator('[data-node="api-client"]').click();
  await expect(f.getByPlaceholder("Add a comment")).toHaveValue("");
});
