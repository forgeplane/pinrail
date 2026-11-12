import { expect, test } from "@playwright/test";
import { clearInbox, core, createReview, pickIn, settingSaved, settingsDialog, shownIn } from "./helpers";

// the layout is a setting the core keeps: each test leaves the default
test.afterEach(async ({ request }) => {
  await request.patch(`${core}/api/v1/settings`, { data: { inbox: { layout: "repository" } } });
  await settingSaved(request, (s) => s.inbox.layout, "repository");
});

test("the inbox is grouped by project or one list, newest first, and remembers which", async ({ page }) => {
  await clearInbox(page.request);
  // oldest first: zeta gets the oldest and the newest, acme the one between
  await createReview(page.request, { title: "Layout one", origin: { repo: "zeta/app", workflow: "layout" } });
  await createReview(page.request, { title: "Layout two", origin: { repo: "acme/api", workflow: "layout" } });
  await createReview(page.request, { title: "Layout three", origin: { repo: "zeta/app", workflow: "layout" } });

  await page.goto("/#/");
  const rows = page.getByTestId("review-row");
  // grouped: the project with the newest review first, then newest within it
  await expect(page.getByTestId("inbox-group-name")).toHaveText(["zeta/app", "acme/api"]);
  await expect(rows).toHaveText([/Layout three/, /Layout one/, /Layout two/]);

  await pickIn(page, "Display options", "Grouping", "None");
  await expect(page.getByTestId("inbox-group")).toHaveCount(0);
  await expect(rows).toHaveText([/Layout three/, /Layout two/, /Layout one/]);
  // with no project headings, each row says its project
  await expect(rows.nth(1).getByTestId("review-row-project")).toHaveText("acme/api");

  // J moves through the rows in the order shown, and wraps around at the ends
  await page.keyboard.press("j");
  await expect(rows.nth(1)).toHaveClass(/is-focused/);
  await page.keyboard.press("j");
  await page.keyboard.press("j");
  await expect(rows.nth(0)).toHaveClass(/is-focused/);
  await page.keyboard.press("k");
  await expect(rows.nth(2)).toHaveClass(/is-focused/);

  // the choice outlasts a reload, once the core has it: a reload while it
  // is being saved would lose it
  await settingSaved(page.request, (s) => s.inbox.layout, "list");
  await page.reload();
  expect(await shownIn(page, "Display options", "Grouping")).toBe("None");
  await expect(rows).toHaveText([/Layout three/, /Layout two/, /Layout one/]);

  await pickIn(page, "Display options", "Grouping", "Repository");
  await expect(page.getByTestId("inbox-group")).toHaveCount(2);
});

test("⌘D discards the focused review, but not from behind a dialog, and D alone does not", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Quiet keys" });
  const discard = page.getByRole("dialog", { name: "Discard this review?" });

  await page.goto("/#/");
  await expect(page.getByTestId("review-row")).toHaveCount(1);
  await page.keyboard.press("ControlOrMeta+,");
  await expect(settingsDialog(page)).toBeVisible();
  await page.keyboard.press("ControlOrMeta+d");
  await expect(discard, "⌘D reached the inbox behind Settings").toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(settingsDialog(page)).toHaveCount(0);

  await page.keyboard.press("d");
  await expect(discard, "D alone opened the discard dialog").toHaveCount(0);
  await page.keyboard.press("ControlOrMeta+d");
  await expect(discard).toBeVisible();
  await expect(discard).toContainText("Quiet keys");
});

test("a row's details stay on one line beside the panel, and only the project shortens", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, {
    title: "Posts announcing the release, with every draft",
    origin: { repo: "a-project-with-a-long-name", workflow: "outreach" },
  });
  await page.setViewportSize({ width: 1100, height: 600 });
  await page.goto("/#/");
  await pickIn(page, "Display options", "Grouping", "None");
  const row = page.getByTestId("review-row");
  await row.click();
  await expect(page.getByTestId("review-panel")).toBeVisible();
  const meta = row.locator(".review-row-meta");
  const lineHeight = await meta.evaluate((el) => parseFloat(getComputedStyle(el).lineHeight) || 16);
  expect((await meta.boundingBox())!.height).toBeLessThan(lineHeight * 1.5);
  const project = row.getByTestId("review-row-project");
  expect(await project.evaluate((el) => el.scrollWidth > el.clientWidth)).toBe(true);
  await expect(meta).toContainText("outreach");
});

test("a row beside the panel keeps its details clear of the plugin, at any width of the list", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, {
    title: "Posts announcing the release on every channel we use",
    origin: { repo: "forgeplane/pinrail", workflow: "outreach", ref: "main" },
  });
  await page.setViewportSize({ width: 1500, height: 700 });
  await page.goto("/#/");
  await pickIn(page, "Display options", "Grouping", "None");
  const row = page.getByTestId("review-row");
  await row.click();
  const panel = page.getByTestId("review-panel");
  await expect(panel).toBeVisible();
  const resizer = page.getByRole("separator", { name: "Resize the review panel" });
  const list = page.locator(".inbox-side");
  // the list from narrow to wide, by the panel's edge
  for (const width of [380, 480, 600, 700, 820]) {
    const edge = (await resizer.boundingBox())!;
    const side = (await list.boundingBox())!;
    const to = edge.x + (side.x + width - (side.x + side.width));
    await page.mouse.move(edge.x + 2, edge.y + 100);
    await page.mouse.down();
    await page.mouse.move(to, edge.y + 100, { steps: 4 });
    await page.mouse.up();
    const at = `with the list ${Math.round((await list.boundingBox())!.width)} pixels wide`;
    // the details under the title end before the plugin's name, when it shows
    const plugin = row.locator(".review-row-plugin");
    if (await plugin.isVisible()) {
      const meta = await row
        .locator(".review-row-meta > *")
        .evaluateAll((els) => Math.max(...els.map((el) => el.getBoundingClientRect().right)));
      expect(meta, at).toBeLessThanOrEqual((await plugin.boundingBox())!.x);
    }
    // the title keeps at least half the row
    const title = (await row.locator(".review-row-main").boundingBox())!;
    expect(title.width, at).toBeGreaterThan((await row.boundingBox())!.width / 2);
    // the sort's label stays on one line
    const sort = page.locator(".list-sort");
    if (await sort.isVisible()) expect((await sort.boundingBox())!.height, at).toBeLessThan(20);
  }
});

test("the row under the pointer keeps its outline off the open row beside it", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Row one", origin: { repo: "acme/api" } });
  await createReview(page.request, { title: "Row two", origin: { repo: "acme/api" } });
  await page.setViewportSize({ width: 1400, height: 700 });
  await page.goto("/#/");
  const rows = page.getByTestId("review-row");
  for (const layout of ["None", "Repository"]) {
    await pickIn(page, "Display options", "Grouping", layout);
    await rows.nth(1).click();
    await expect(page.getByTestId("review-panel")).toBeVisible();
    // the pointer moving onto a row puts the focus on it, once it has moved
    // since the review opened
    const box = (await rows.nth(0).boundingBox())!;
    await page.mouse.move(box.x + 20, box.y - 40, { steps: 3 });
    await page.mouse.move(box.x + 20, box.y + box.height / 2, { steps: 3 });
    await expect(rows.nth(0)).toHaveClass(/is-focused/);
    // how far the focused row's outline reaches past its box
    const reach = await rows.nth(0).evaluate((el) => {
      const s = getComputedStyle(el);
      return s.outlineStyle === "none" ? 0 : parseFloat(s.outlineWidth) + parseFloat(s.outlineOffset);
    });
    const focused = (await rows.nth(0).boundingBox())!;
    const open = (await rows.nth(1).boundingBox())!;
    expect(focused.y + focused.height + reach, layout).toBeLessThanOrEqual(open.y);
  }
});

for (const from of ["the review", "its row in the inbox"])
  test(`discarding the open review from ${from} opens the next one, as a hand-over does`, async ({ page }) => {
    await clearInbox(page.request);
    // newest first in the list: Three, Two, One
    for (const title of ["Discard one", "Discard two", "Discard three"])
      await createReview(page.request, { title, origin: { repo: "acme/api" } });
    await page.setViewportSize({ width: 1400, height: 800 });
    await page.goto("/#/");
    await pickIn(page, "Display options", "Grouping", "None");
    const rows = page.getByTestId("review-row");
    await rows.filter({ hasText: "Discard two" }).click();
    const panel = page.getByTestId("review-panel");
    await expect(panel.getByTestId("review-title")).toHaveText("Discard two");
    if (from === "the review") await panel.getByRole("button", { name: "Discard this review" }).click();
    else await rows.filter({ hasText: "Discard two" }).getByRole("button", { name: "Discard Discard two" }).click();
    const dialog = page.getByRole("dialog", { name: "Discard this review?" });
    await dialog.getByRole("button", { name: "Discard", exact: true }).click();
    await expect(dialog).toHaveCount(0);
    // the review after it in the list, open in the panel, and the discarded one gone
    await expect(panel.getByTestId("review-title")).toHaveText("Discard one");
    await expect(rows.filter({ hasText: "Discard two" })).toHaveCount(0);
    await expect(rows.filter({ hasText: "Discard one" })).toHaveClass(/is-open/);
  });

test("a group's heading has an arrow that folds the group and turns with it", async ({ page }) => {
  await clearInbox(page.request);
  await createReview(page.request, { title: "Fold: api", origin: { repo: "fold/api" } });
  await createReview(page.request, { title: "Fold: web", origin: { repo: "fold/web" } });
  await page.goto("/#/");
  const group = page.getByTestId("inbox-group").filter({ hasText: "fold/api" });
  const arrow = group.locator(".inbox-group-fold");
  await expect(arrow).toBeVisible();
  const turn = () => arrow.evaluate((el) => getComputedStyle(el).transform);
  expect(await turn()).toBe("none");

  await group.getByTestId("inbox-group-heading").click();
  await expect(group.getByTestId("review-row")).toBeHidden();
  // a quarter turn, to point right
  await expect.poll(turn).toMatch(/^matrix\((-?0(\.0+)?(e-\d+)?), -1, 1, (-?0(\.0+)?(e-\d+)?), 0, 0\)$/);

  await group.getByTestId("inbox-group-heading").click();
  await expect(group.getByTestId("review-row")).toBeVisible();
  await expect.poll(turn).toBe("none");

  // the arrow lines up with the icon beside it
  const [a, b] = await group
    .getByTestId("inbox-group-heading")
    .evaluate((h) =>
      [...h.querySelectorAll("svg")]
        .slice(0, 2)
        .map((s) => s.getBoundingClientRect().top + s.getBoundingClientRect().height / 2),
    );
  expect(Math.abs(a - b)).toBeLessThan(0.5);
});
