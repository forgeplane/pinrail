// The Projects page: projects made and changed in the app, listed by name,
// each with its repositories, and the repositories in no project last.

import { expect, test, type APIRequestContext, type Locator, type Page } from "@playwright/test";
import { clearInbox, core, createReview, pickIn, settingSaved } from "./helpers";

/** Deletes every project, so each test starts with none. */
async function noProjects(request: APIRequestContext) {
  const { projects } = (await (await request.get(`${core}/api/v1/projects`)).json()) as { projects: { id: string }[] };
  for (const p of projects) await request.delete(`${core}/api/v1/projects/${p.id}`);
}

const card = (page: Page, name: string) => page.getByRole("region", { name, exact: true });

/** Drags a repository's row onto a project, as the browser's drag and drop
 *  does, with the page's own handlers. A drag with the mouse picks up the
 *  row under the pointer after the page scrolls to the target, which in a
 *  long list is another row. */
async function drag(source: Locator, target: Locator) {
  const transfer = await source.page().evaluateHandle(() => new DataTransfer());
  // the row picked up, held on to: after the drop the page shows it in its
  // new place, and the drag ends on the element that was dragged
  const dragged = await source.elementHandle();
  await source.dispatchEvent("dragstart", { dataTransfer: transfer });
  await target.dispatchEvent("dragenter", { dataTransfer: transfer });
  await target.dispatchEvent("dragover", { dataTransfer: transfer });
  await target.dispatchEvent("drop", { dataTransfer: transfer });
  await dragged?.dispatchEvent("dragend", { dataTransfer: transfer });
}
const loose = (page: Page) => page.getByRole("region", { name: "Repositories in no project" });

test.beforeEach(({ request }) => noProjects(request));
test.afterAll(({ request }) => noProjects(request));
// the inbox's layout is a setting the core keeps: each test leaves the default
test.afterEach(async ({ request }) => {
  await request.patch(`${core}/api/v1/settings`, {
    data: { inbox: { layout: "repository" }, sessions: { layout: "list" } },
  });
  await settingSaved(request, (s) => [s.inbox.layout, s.sessions.layout], ["repository", "list"]);
});

test("a project is created with a description, edited, and deleted", async ({ page }) => {
  await createReview(page.request, { title: "Projects: app", origin: { repo: "proj/app" } });
  await createReview(page.request, { title: "Projects: infra", origin: { repo: "proj/infra" } });

  await page.goto("/#/projects");
  await expect(page.getByRole("heading", { name: "Group repositories into projects" })).toBeVisible();
  await expect(loose(page)).toContainText("proj/app");
  await page.getByRole("button", { name: "New project" }).click();

  // a name and a description; the repositories come by dragging, not from a list
  const dialog = page.getByRole("dialog", { name: "New project" });
  await expect(dialog.getByRole("checkbox")).toHaveCount(0);
  await dialog.getByLabel("Name", { exact: true }).fill("Proj");
  await dialog.getByLabel("Description (optional)").fill("The app and its infrastructure");
  await dialog.getByRole("button", { name: "Create project" }).click();
  await expect(dialog).toBeHidden();
  await expect(card(page, "Proj")).toContainText("The app and its infrastructure");
  await expect(card(page, "Proj")).toContainText("Drop a repository here");

  for (const repo of ["proj/app", "proj/infra"])
    await drag(loose(page).getByTestId("project-repo").filter({ hasText: repo }), card(page, "Proj"));
  await expect(card(page, "Proj").getByTestId("project-repo")).toHaveText([/proj\/app/, /proj\/infra/]);
  await expect(card(page, "Proj")).toContainText("2 waiting");

  // a name another project has is refused, whatever its case
  await page.getByRole("button", { name: "New project" }).click();
  await dialog.getByLabel("Name", { exact: true }).fill("proj");
  await dialog.getByRole("button", { name: "Create project" }).click();
  await expect(dialog).toContainText("another project has this name");
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();

  // Edit changes the name and the description, and an empty one removes it
  await card(page, "Proj").getByRole("button", { name: "Proj actions" }).click();
  await page.getByRole("menuitem", { name: "Edit…" }).click();
  const edit = page.getByRole("dialog", { name: "Edit project" });
  await expect(edit.getByLabel("Description (optional)")).toHaveValue("The app and its infrastructure");
  await edit.getByLabel("Name", { exact: true }).fill("Proj app");
  await edit.getByLabel("Description (optional)").fill("");
  await edit.getByRole("button", { name: "Save" }).click();
  await expect(card(page, "Proj app")).toBeVisible();
  await expect(card(page, "Proj app")).not.toContainText("The app and its infrastructure");

  await card(page, "Proj app").getByRole("button", { name: "Proj app actions" }).click();
  await page.getByRole("menuitem", { name: "Delete project…" }).click();
  await page.getByRole("dialog", { name: "Delete Proj app?" }).getByRole("button", { name: "Delete project" }).click();
  await expect(page.getByTestId("project-group")).toHaveCount(0);
  // its repositories are in no project again
  await expect(loose(page)).toContainText("proj/infra");
});

test("a project started from a repository's menu begins with it", async ({ page, request }) => {
  await createReview(request, { title: "Projects: seed", origin: { repo: "proj/seed" } });
  await page.goto("/#/projects");
  const seed = loose(page).getByTestId("project-repo").filter({ hasText: "proj/seed" });
  await seed.hover();
  await seed.getByRole("button", { name: "proj/seed actions" }).click();
  await page.getByRole("menuitem", { name: "New project…" }).click();
  const dialog = page.getByRole("dialog", { name: "New project" });
  await expect(dialog).toContainText("It starts with proj/seed");
  await dialog.getByLabel("Name", { exact: true }).fill("Seeded");
  await dialog.getByRole("button", { name: "Create project" }).click();
  await expect(card(page, "Seeded").getByTestId("project-repo")).toHaveText([/proj\/seed/]);
});

test("a repository moves between projects by dragging and from its menu, and only so", async ({ page, request }) => {
  await createReview(request, { title: "Projects: web", origin: { repo: "proj/web" } });
  for (const name of ["One", "Two"]) {
    const response = await request.post(`${core}/api/v1/projects`, { data: { name } });
    expect(response.status()).toBe(201);
  }
  await page.goto("/#/projects");
  const web = loose(page).getByTestId("project-repo").filter({ hasText: "proj/web" });

  // dragged onto a card, it goes in that project
  await drag(web, card(page, "One"));
  await expect(card(page, "One")).toContainText("proj/web");
  await expect(loose(page).getByText("proj/web")).toHaveCount(0);

  // its menu moves it to another, and takes it out of every project
  const inOne = card(page, "One").getByTestId("project-repo").filter({ hasText: "proj/web" });
  await inOne.hover();
  await inOne.getByRole("button", { name: "proj/web actions" }).click();
  await page.getByRole("menuitem", { name: "Two" }).click();
  await expect(card(page, "Two")).toContainText("proj/web");
  await expect(card(page, "One")).not.toContainText("proj/web");

  const inTwo = card(page, "Two").getByTestId("project-repo").filter({ hasText: "proj/web" });
  await inTwo.hover();
  await inTwo.getByRole("button", { name: "proj/web actions" }).click();
  await page.getByRole("menuitem", { name: "Remove from Two" }).click();
  await expect(loose(page)).toContainText("proj/web");

  // repositories come by dragging: a project's heading offers no other way
  await expect(card(page, "One").getByRole("button", { name: /Add a repository/ })).toHaveCount(0);
});

test("the sidebar leads to the Projects page", async ({ page }) => {
  await page.goto("/#/");
  await page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: "Projects" }).click();
  await expect(page).toHaveURL(/#\/projects$/);
  await expect(page.getByRole("heading", { name: "Projects", level: 1 })).toBeVisible();
});

test("the projects are listed by name, and stay in place while repositories move", async ({ page, request }) => {
  await createReview(request, { title: "Keep: a", origin: { repo: "keep/a" } });
  await createReview(request, { title: "Keep: b", origin: { repo: "keep/b" } });
  // a second later, in whole seconds as reviews are stamped: newer than the others
  await page.waitForTimeout(1100);
  await createReview(request, { title: "Keep: c", origin: { repo: "keep/c" } });
  await request.post(`${core}/api/v1/projects`, { data: { name: "quiet", repos: [] } });
  await request.post(`${core}/api/v1/projects`, { data: { name: "Busy", repos: ["keep/a", "keep/b"] } });
  await page.goto("/#/projects");
  const names = page.getByTestId("project-group").locator("h2");
  await expect(names).toHaveText(["Busy", "quiet"]);
  await expect(card(page, "quiet")).toContainText("Drop a repository here");

  // the newest activity moving into the last project leaves it where it was
  await drag(loose(page).getByTestId("project-repo").filter({ hasText: "keep/c" }), card(page, "quiet"));
  await expect(card(page, "quiet")).toContainText("keep/c");
  await expect(names).toHaveText(["Busy", "quiet"]);

  // a project folds, and the filter keeps the projects and repositories that match
  await card(page, "Busy").getByRole("button", { name: "Fold Busy" }).click();
  await expect(card(page, "Busy").getByTestId("project-repo")).toHaveCount(0);
  await card(page, "Busy").getByRole("button", { name: "Unfold Busy" }).click();
  await page.getByLabel("Filter projects and repositories").fill("keep/b");
  await expect(page.getByTestId("project-repo")).toHaveText([/keep\/b/]);
  await expect(names).toHaveText(["Busy"]);
});

test("the inbox groups and filters by project, and names each review's repository in a mixed one", async ({
  page,
  request,
}) => {
  await clearInbox(request);
  await createReview(request, { title: "Inbox: app", origin: { repo: "mix/app" } });
  await createReview(request, { title: "Inbox: infra", origin: { repo: "mix/infra" } });
  await createReview(request, { title: "Inbox: elsewhere", origin: { repo: "mix/elsewhere" } });
  const created = await request.post(`${core}/api/v1/projects`, {
    data: { name: "Mix", repos: ["mix/app", "mix/infra"] },
  });
  const { id } = (await created.json()) as { id: string };

  await page.goto("/#/");
  await pickIn(page, "Display options", "Grouping", "Project");
  const groups = page.getByTestId("inbox-group-name");
  await expect(groups).toHaveText(["mix/elsewhere", "Mix"]);
  const mixed = page.getByTestId("inbox-group").filter({ has: page.getByText("Mix", { exact: true }) });
  await expect(mixed.getByTestId("review-row-project")).toHaveText(["mix/infra", "mix/app"]);
  // each heading's arrow, and its icon or avatar, is centred on the name's capital letters
  for (const heading of await page.getByTestId("inbox-group-heading").all()) {
    const offsets = await heading.evaluate((h) => {
      const name = h.querySelector("strong")!;
      // the baseline, where an empty inline block sits, and the capitals' height
      const probe = document.createElement("span");
      probe.style.cssText = "display: inline-block; width: 0; height: 0";
      name.appendChild(probe);
      const baseline = probe.getBoundingClientRect().bottom;
      probe.remove();
      const context = document.createElement("canvas").getContext("2d")!;
      context.font = getComputedStyle(name).font;
      const capCentre = baseline - context.measureText("P").actualBoundingBoxAscent / 2;
      return [...h.querySelectorAll(":scope > svg, :scope > .project-avatar")].map((icon) => {
        const box = icon.getBoundingClientRect();
        return box.top + box.height / 2 - capCentre;
      });
    });
    expect(offsets.length).toBe(2);
    for (const offset of offsets) expect(Math.abs(offset)).toBeLessThan(0.3);
  }

  // the filter takes the project, and the address keeps it
  await pickIn(page, "Filter", "Project or repository", "Mix");
  await expect(page).toHaveURL(new RegExp(`project=${id}`));
  await expect(page.getByTestId("review-row")).toHaveCount(2);

  // the history filters by it too
  await page.getByRole("tab", { name: "History" }).click();
  await expect(page).toHaveURL(new RegExp(`project=${id}`));

  // the layout is kept in the settings
  await settingSaved(request, (s) => s.inbox.layout, "project");
});

test("the project or repository filter narrows its choices to what is typed", async ({ page, request }) => {
  await clearInbox(request);
  await createReview(request, { title: "Type: app", origin: { repo: "typed/app" } });
  await createReview(request, { title: "Type: web", origin: { repo: "typed/web" } });
  await request.post(`${core}/api/v1/projects`, { data: { name: "Typed", repos: ["typed/app"] } });
  await page.goto("/#/");
  const filter = page.getByRole("button", { name: "Filter", exact: true });
  const openScope = async () => {
    await filter.click();
    await page.getByRole("menuitem", { name: /^Project or repository/ }).click();
  };
  const choices = page.getByRole("menu", { name: "Project or repository" }).getByRole("menuitemradio");

  // the submenu opens with its search focused, and narrows to what is typed
  await openScope();
  const search = page.getByRole("textbox", { name: "Filter project or repository" });
  await expect(search).toBeFocused();
  await search.fill("we");
  await expect(choices.locator(".menu-label")).toHaveText(["typed/web"]);

  // the projects match by their names too; the arrow keys and Enter pick a match
  await search.fill("TYPED");
  await expect(choices.locator(".menu-label")).toHaveText(["Typed", "typed/app", "typed/web"]);
  await search.press("ArrowDown");
  await search.press("Enter");
  await expect(page).toHaveURL(/repo=typed%2Fapp/);
  await expect(page.getByTestId("review-row")).toHaveText([/Type: app/]);

  // a pick leaves the menu open for another filter, until Escape
  await page.keyboard.press("Escape");
  // nothing matching says so, and Escape leaves the choice as it was
  await openScope();
  await expect(search).toHaveValue("");
  await search.fill("nothing like it");
  await expect(page.getByRole("menu", { name: "Project or repository" })).toContainText("No matches");
  await search.press("Escape");
  await expect(page.getByRole("menu", { name: "Project or repository" })).toHaveCount(0);
  await expect(filter).toBeFocused();
  await expect(page).toHaveURL(/repo=typed%2Fapp/);
});

test("a project's avatar takes a colour and an icon, and shows where the project does", async ({ page, request }) => {
  await clearInbox(request);
  await createReview(request, { title: "Avatar: app", origin: { repo: "avatar/app" } });
  await request.post(`${core}/api/v1/projects`, { data: { name: "Avatar", repos: ["avatar/app"] } });
  await page.goto("/#/projects");
  const group = card(page, "Avatar");
  // the first letter on a colour of its own until the person chooses
  await expect(group.locator(".project-avatar")).toHaveText("A");

  await group.getByRole("button", { name: "Avatar actions" }).click();
  await page.getByRole("menuitem", { name: "Edit…" }).click();
  const edit = page.getByRole("dialog", { name: "Edit project" });
  // sixteen colours, each drawn in a colour of its own
  const swatches = edit.getByRole("radio");
  await expect(swatches).toHaveCount(16);
  const drawn = await swatches.evaluateAll((all) => all.map((s) => getComputedStyle(s).backgroundColor));
  expect(new Set(drawn).size).toBe(16);
  expect(drawn).not.toContain("rgba(0, 0, 0, 0)");
  await edit.getByRole("radio", { name: "Green" }).click();
  // up to three letters, which shrink to fit the square
  await edit.getByLabel("Icon").fill("pinrail");
  await expect(edit.getByLabel("Icon")).toHaveValue("pin");
  const preview = edit.locator(".project-avatar");
  await expect(preview).toHaveText("PIN");
  // letters stay white on the solid colour, as the name's first letter does
  await expect(preview).not.toHaveClass(/is-emoji/);
  const [text, square] = await preview.evaluate((a) => [
    a.querySelector("text")!.getBoundingClientRect().width,
    a.getBoundingClientRect().width,
  ]);
  expect(text).toBeLessThan(square);
  await edit.getByLabel("Icon").fill("🚂");
  await edit.getByRole("button", { name: "Save" }).click();
  await expect(group.locator(".project-avatar")).toHaveText("🚂");
  await expect(group.locator(".project-avatar")).toHaveAttribute("data-color", "green");

  // the letter sits in the middle of its square
  await edit.waitFor({ state: "detached" });
  await group.getByRole("button", { name: "Avatar actions" }).click();
  await page.getByRole("menuitem", { name: "Edit…" }).click();
  await edit.getByLabel("Icon").fill("");
  await edit.getByRole("button", { name: "Save" }).click();
  await expect(group.locator(".project-avatar")).toHaveText("A");
  // the letter's baseline is half the font's capital height below the middle,
  // with the height measured from the font as drawn
  const offset = await group.locator(".project-avatar").evaluate((avatar) => {
    const text = avatar.querySelector("text")!;
    const svg = avatar.querySelector("svg")!.getBoundingClientRect();
    const scale = svg.height / 100;
    const size = Number(text.getAttribute("font-size")) * scale;
    const baseline =
      svg.top +
      (Number(text.getAttribute("y")) + parseFloat(text.getAttribute("dy")!) * Number(text.getAttribute("font-size"))) *
        scale;
    const context = document.createElement("canvas").getContext("2d")!;
    context.font = `${getComputedStyle(text).fontWeight} ${size}px ${getComputedStyle(text).fontFamily}`;
    const cap = context.measureText("A").actualBoundingBoxAscent;
    return baseline - cap / 2 - (svg.top + svg.height / 2);
  });
  expect(Math.abs(offset)).toBeLessThan(0.3);

  // the inbox's group shows it too, at the size the Projects page does
  const onPage = (await group.locator(".project-avatar").boundingBox())!;
  await page.goto("/#/");
  await pickIn(page, "Display options", "Grouping", "Project");
  const inInbox = page.getByTestId("inbox-group").filter({ hasText: "Avatar" }).locator(".project-avatar");
  await expect(inInbox).toHaveAttribute("data-color", "green");
  const inbox = (await inInbox.boundingBox())!;
  expect([inbox.width, inbox.height]).toEqual([onPage.width, onPage.height]);
});

test("at the large interface size, a project's menu opens under its button", async ({ page, request }) => {
  await request.post(`${core}/api/v1/projects`, { data: { name: "Zoomed" } });
  await request.patch(`${core}/api/v1/settings`, { data: { appearance: { text_size: "large" } } });
  try {
    await page.setViewportSize({ width: 1600, height: 900 });
    await page.goto("/#/projects");
    const button = card(page, "Zoomed").getByRole("button", { name: "Zoomed actions" });
    await expect(page.locator("#root")).toHaveCSS("zoom", "1.08");
    await button.click();
    const menu = page.getByRole("menu", { name: "Zoomed" });
    await expect(menu).toBeVisible();
    const b = (await button.boundingBox())!;
    const m = (await menu.boundingBox())!;
    // its right edge on the button's, just below it
    expect(Math.abs(m.x + m.width - (b.x + b.width))).toBeLessThan(2);
    expect(m.y - (b.y + b.height)).toBeGreaterThanOrEqual(0);
    expect(m.y - (b.y + b.height)).toBeLessThan(10);
  } finally {
    await request.patch(`${core}/api/v1/settings`, { data: { appearance: { text_size: "default" } } });
    await settingSaved(request, (s) => s.appearance.text_size, "default");
  }
});

test("the command palette goes to the projects, makes one, and shows one's reviews", async ({ page, request }) => {
  const created = await request.post(`${core}/api/v1/projects`, { data: { name: "Palette" } });
  const { id } = (await created.json()) as { id: string };
  await page.goto("/#/");
  const palette = page.getByRole("dialog", { name: "Search" });
  const run = async (text: string, option: RegExp) => {
    await page.keyboard.press("ControlOrMeta+k");
    await palette.getByRole("textbox").fill(text);
    await expect(palette.getByRole("option", { name: option })).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("Enter");
  };

  await run("go to projects", /Go to projects/);
  await expect(page).toHaveURL(/#\/projects$/);

  await run("new project", /New project/);
  await expect(page.getByRole("dialog", { name: "New project" })).toBeVisible();
  await page.keyboard.press("Escape");

  await run("show palette", /Show Palette in the inbox/);
  await expect(page).toHaveURL(new RegExp(`project=${id}`));
});

test("a project's avatar takes a PNG, cropped to its middle square, and gives it up again", async ({
  page,
  request,
}) => {
  await clearInbox(request);
  await createReview(request, { title: "Image: app", origin: { repo: "image/app" } });
  await request.post(`${core}/api/v1/projects`, { data: { name: "Pictured", repos: ["image/app"] } });
  await page.goto("/#/projects");
  // a 400 by 300 PNG, drawn here
  const png = Buffer.from(
    (
      await page.evaluate(() => {
        const canvas = document.createElement("canvas");
        canvas.width = 400;
        canvas.height = 300;
        const context = canvas.getContext("2d")!;
        context.fillStyle = "#c33";
        context.fillRect(0, 0, 400, 300);
        return canvas.toDataURL("image/png");
      })
    ).split(",")[1],
    "base64",
  );
  const group = card(page, "Pictured");
  const edit = page.getByRole("dialog", { name: "Edit project" });

  // a file that is not a PNG is refused
  await group.getByRole("button", { name: "Pictured actions" }).click();
  await page.getByRole("menuitem", { name: "Edit…" }).click();
  await edit
    .getByLabel("Image")
    .setInputFiles({ name: "photo.gif", mimeType: "image/gif", buffer: Buffer.from("GIF89a") });
  await expect(edit).toContainText("Choose a PNG image.");

  await edit.getByLabel("Image").setInputFiles({ name: "photo.png", mimeType: "image/png", buffer: png });
  await expect(edit.locator(".project-avatar img")).toBeVisible();
  await edit.getByRole("button", { name: "Save" }).click();
  await expect(edit).toBeHidden();

  // shown on the page and in the inbox, as the middle square at 256 pixels
  const shown = group.locator(".project-avatar img");
  await expect(shown).toHaveAttribute("src", /\/api\/v1\/projects\/p_\w+\/image\?v=[0-9a-f]{64}$/);
  const size = await shown.evaluate(async (img: HTMLImageElement) => {
    await img.decode();
    return [img.naturalWidth, img.naturalHeight];
  });
  expect(size).toEqual([256, 256]);
  await page.goto("/#/");
  await pickIn(page, "Display options", "Grouping", "Project");
  await expect(
    page.getByTestId("inbox-group").filter({ hasText: "Pictured" }).locator(".project-avatar img"),
  ).toBeVisible();

  // removed, the letter is back
  await page.goto("/#/projects");
  await group.getByRole("button", { name: "Pictured actions" }).click();
  await page.getByRole("menuitem", { name: "Edit…" }).click();
  await edit.getByRole("button", { name: "Remove image" }).click();
  await edit.getByRole("button", { name: "Save" }).click();
  await expect(group.locator(".project-avatar")).toHaveText("P");
  await expect(group.locator(".project-avatar img")).toHaveCount(0);
});

test("the Sessions page groups the sessions by project or by repository", async ({ page, request }) => {
  const run = Date.now();
  const report = (id: string, repo: string, text: string) =>
    request.post(`${core}/api/v1/sessions/claude-code/${id}-${run}/status`, {
      data: { text, kind: "start", origin: { repo } },
    });
  await report("grp-app", `grp-app/${run}`, "App work");
  await report("grp-infra", `grp-infra/${run}`, "Infra work");
  await report("grp-other", `grp-other/${run}`, "Other work");
  const name = `Grouped ${run}`;
  await request.post(`${core}/api/v1/projects`, {
    data: { name, repos: [`grp-app/${run}`, `grp-infra/${run}`] },
  });

  // only this run's sessions, which others waiting would push off the first page
  await page.goto(`/#/sessions?q=${run}`);
  await expect(page.getByTestId("session-row")).toHaveCount(3);
  await expect(page.getByTestId("inbox-group")).toHaveCount(0);
  await pickIn(page, "Display options", "Grouping", "Project");
  const group = (title: string) =>
    page
      .getByTestId("inbox-group")
      .filter({ has: page.getByTestId("inbox-group-name").getByText(title, { exact: true }) });
  await expect(group(name).getByTestId("session-row")).toHaveCount(2);
  // in a project of several repositories, each session names its own
  await expect(group(name).locator(".review-row-project")).toHaveCount(2);
  await expect(group(`grp-other/${run}`).getByTestId("session-row")).toHaveCount(1);

  // by repository, each has its group, and the rows leave the repository out
  await pickIn(page, "Display options", "Grouping", "Repository");
  await expect(group(`grp-app/${run}`).getByTestId("session-row")).toHaveCount(1);
  await expect(group(`grp-app/${run}`).locator(".review-row-project")).toHaveCount(0);
  await settingSaved(request, (s) => s.sessions.layout, "repository");
});

test("Collapse all folds every group on the Projects page, and Expand all opens them", async ({ page, request }) => {
  await createReview(request, { title: "Fold: one", origin: { repo: "fold-all/one" } });
  await createReview(request, { title: "Fold: loose", origin: { repo: "fold-all/loose" } });
  await request.post(`${core}/api/v1/projects`, { data: { name: "Folding", repos: ["fold-all/one"] } });
  await page.goto("/#/projects");
  const rows = page.getByTestId("project-repo");
  await expect(rows.first()).toBeVisible();

  await page.getByRole("button", { name: "Collapse all" }).click();
  await expect(rows).toHaveCount(0);
  // the repositories in no project fold with the projects
  await expect(page.getByRole("button", { name: "Unfold In no project" })).toBeVisible();

  await page.getByRole("button", { name: "Expand all" }).click();
  await expect(card(page, "Folding").getByTestId("project-repo")).toHaveText([/fold-all\/one/]);
  await expect(loose(page)).toContainText("fold-all/loose");
  await expect(page.getByRole("button", { name: "Collapse all" })).toBeVisible();
});

test("search fields neither complete, correct, capitalise nor spell-check what is typed", async ({ page }) => {
  const plain = { autocomplete: "off", autocorrect: "off", autocapitalize: "off", spellcheck: "false" };
  const check = async (field: import("@playwright/test").Locator) => {
    for (const [name, value] of Object.entries(plain)) await expect(field).toHaveAttribute(name, value);
  };
  await page.goto("/#/projects");
  await check(page.getByLabel("Filter projects and repositories"));
  await page.goto("/#/");
  await check(page.getByLabel("Search inbox"));
  await page.getByRole("button", { name: "Filter", exact: true }).click();
  await page.getByRole("menuitem", { name: /^Project or repository/ }).click();
  await check(page.getByRole("textbox", { name: "Filter project or repository" }));
  await page.keyboard.press("Escape");
  await page.goto("/#/sessions");
  await check(page.getByLabel("Search sessions"));
  await page.keyboard.press("ControlOrMeta+k");
  await check(page.getByRole("dialog", { name: "Search" }).getByRole("textbox"));
});

test("the / key focuses the search field of every page that has one", async ({ page }) => {
  const slash = async (field: import("@playwright/test").Locator) => {
    await expect(field).toBeVisible();
    await page.locator("body").click({ position: { x: 600, y: 5 } });
    await expect(field).not.toBeFocused();
    await page.keyboard.press("/");
    await expect(field).toBeFocused();
    await expect(field).toHaveValue("");
  };
  await page.goto("/#/projects");
  await slash(page.getByLabel("Filter projects and repositories"));
  await page.goto("/#/");
  await slash(page.getByLabel("Search inbox"));
  await page.goto("/#/?tab=history");
  await slash(page.getByRole("searchbox"));
  await page.goto("/#/sessions");
  await slash(page.getByLabel("Search sessions"));
  await page.goto("/#/plugins");
  const source = page.getByTestId("install-panel").getByLabel("Source");
  await expect(source).toBeEnabled();
  await page.getByTestId("install-panel").click({ position: { x: 2, y: 60 } });
  await source.blur();
  await page.keyboard.press("/");
  await expect(source).toBeFocused();
  await expect(source).toHaveValue("");
});

test("a repository's menu opens it in the inbox, its sessions and the history", async ({ page, request }) => {
  const run = Date.now();
  const waiting = `open-menu/waiting-${run}`;
  const quiet = `open-menu/quiet-${run}`;
  await createReview(request, { title: "Open: waiting", origin: { repo: waiting } });
  await request.post(`${core}/api/v1/sessions/claude-code/open-${run}/status`, {
    data: { text: "Quiet work", kind: "start", origin: { repo: quiet } },
  });
  await page.goto("/#/projects");
  const menuOf = async (repo: string) => {
    const row = loose(page).getByTestId("project-repo").filter({ hasText: repo });
    await row.hover();
    await row.getByRole("button", { name: `${repo} actions` }).click();
    return page.getByRole("menu", { name: repo });
  };

  // a repository with something waiting opens in the inbox, filtered to it
  let menu = await menuOf(waiting);
  await menu.getByRole("menuitem", { name: "Open in the inbox" }).click();
  await expect(page).toHaveURL(new RegExp(`repo=${encodeURIComponent(waiting)}`));
  await expect(page.getByTestId("review-row")).toHaveText([/Open: waiting/]);

  // one with nothing waiting cannot, but its sessions and its history can
  await page.goto("/#/projects");
  menu = await menuOf(quiet);
  await expect(menu.getByRole("menuitem", { name: "Open in the inbox" })).toBeDisabled();
  await menu.getByRole("menuitem", { name: "Open its sessions" }).click();
  await expect(page).toHaveURL(new RegExp(`#/sessions\\?repo=${encodeURIComponent(quiet)}`));
  await expect(page.getByTestId("session-row")).toHaveCount(1);

  await page.goto("/#/projects");
  menu = await menuOf(quiet);
  await menu.getByRole("menuitem", { name: "Open in the history" }).click();
  await expect(page).toHaveURL(new RegExp(`tab=history&repo=${encodeURIComponent(quiet)}`));
});

test("a menu near the window's bottom opens above its button", async ({ page, request }) => {
  await request.post(`${core}/api/v1/projects`, { data: { name: "Low" } });
  await page.setViewportSize({ width: 1200, height: 260 });
  await page.goto("/#/projects");
  const button = card(page, "Low").getByRole("button", { name: "Low actions" });
  await button.evaluate((b) => b.scrollIntoView({ block: "end" }));
  await button.click();
  const menu = page.getByRole("menu", { name: "Low" });
  await expect(menu).toBeInViewport({ ratio: 1 });
  const b = (await button.boundingBox())!;
  const m = (await menu.boundingBox())!;
  // it rises from the button rather than falling past the window's bottom
  expect(m.y).toBeLessThan(b.y);
});

test("deleting a repository discards what waits and deletes its reviews and sessions", async ({ page, request }) => {
  const run = Date.now();
  const repo = `delete-me/repo-${run}`;
  await createReview(request, { title: `Doomed ${run}`, origin: { repo } });
  await request.post(`${core}/api/v1/sessions/claude-code/doomed-${run}/status`, {
    data: { text: "Doomed work", kind: "start", origin: { repo } },
  });
  await page.goto("/#/projects");
  const row = loose(page).getByTestId("project-repo").filter({ hasText: repo });
  await row.hover();
  await row.getByRole("button", { name: `${repo} actions` }).click();
  await page.getByRole("menu", { name: repo }).getByRole("menuitem", { name: "Delete repository…" }).click();

  const dialog = page.getByRole("dialog", { name: `Delete ${repo}?` });
  await expect(dialog.getByTestId("repo-delete-waiting")).toContainText("1 review or question still waits");
  // the button waits for the repository's name, typed out in full
  const confirm = dialog.getByRole("button", { name: "Delete repository" });
  await expect(confirm).toBeDisabled();
  await dialog.getByLabel(/To confirm, type/).fill(repo.slice(0, -1));
  await expect(confirm).toBeDisabled();
  await dialog.getByLabel(/To confirm, type/).fill(repo);
  await confirm.click();
  await expect(dialog).toBeHidden();
  await expect(row).toHaveCount(0);

  // nothing of it is left in the inbox, the history or the sessions
  const { reviews } = await (
    await request.get(
      `${core}/api/v1/reviews?repo=${encodeURIComponent(repo)}&status=pending,decided,discarded,withdrawn,expired`,
    )
  ).json();
  expect(reviews).toEqual([]);
  const session = await request.get(`${core}/api/v1/sessions/claude-code/doomed-${run}`);
  expect(session.status()).toBe(404);
  await page.goto(`/#/?q=${run}`);
  await expect(page.getByTestId("review-row")).toHaveCount(0);
});

test("the Projects page drops a deleted repository without waiting for its event", async ({ page, request }) => {
  const repo = `delete-quiet/repo-${Date.now()}`;
  await createReview(request, { title: "Quiet delete", origin: { repo } });
  // a window whose event stream does not carry the deletion's event
  await page.route("**/api/v1/events*", (route) => route.abort());
  await page.goto("/#/projects");
  const row = loose(page).getByTestId("project-repo").filter({ hasText: repo });
  await row.hover();
  await row.getByRole("button", { name: `${repo} actions` }).click();
  await page.getByRole("menu", { name: repo }).getByRole("menuitem", { name: "Delete repository…" }).click();
  const dialog = page.getByRole("dialog", { name: `Delete ${repo}?` });
  await dialog.getByLabel(/To confirm, type/).fill(repo);
  await dialog.getByRole("button", { name: "Delete repository" }).click();
  await expect(row).toHaveCount(0);
});

test("the Projects page shows a moved repository without waiting for its event", async ({ page, request }) => {
  const repo = `move-quiet/repo-${Date.now()}`;
  await createReview(request, { title: "Quiet move", origin: { repo } });
  expect((await request.post(`${core}/api/v1/projects`, { data: { name: "Quiet" } })).status()).toBe(201);
  // a window whose event stream does not carry the move's event
  await page.route("**/api/v1/events*", (route) => route.abort());
  await page.goto("/#/projects");
  const row = loose(page).getByTestId("project-repo").filter({ hasText: repo });
  await row.hover();
  await row.getByRole("button", { name: `${repo} actions` }).click();
  await page.getByRole("menu", { name: repo }).getByRole("menuitem", { name: "Quiet" }).click();
  await expect(card(page, "Quiet")).toContainText(repo);
  await expect(row).toHaveCount(0);
});
