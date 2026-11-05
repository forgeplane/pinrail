// The Projects page: projects made and changed in the app, listed by name,
// each with its repositories, and the repositories in no project last.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { clearInbox, core, createReview, settingSaved } from "./helpers";

/** Deletes every project, so each test starts with none. */
async function noProjects(request: APIRequestContext) {
  const { projects } = (await (await request.get(`${core}/api/v1/projects`)).json()) as { projects: { id: string }[] };
  for (const p of projects) await request.delete(`${core}/api/v1/projects/${p.id}`);
}

const card = (page: Page, name: string) => page.getByRole("region", { name, exact: true });
const loose = (page: Page) => page.getByRole("region", { name: "Repositories in no project" });

test.beforeEach(({ request }) => noProjects(request));
test.afterAll(({ request }) => noProjects(request));
// the inbox's layout is a setting the core keeps: each test leaves the default
test.afterEach(async ({ request }) => {
  await request.patch(`${core}/api/v1/settings`, { data: { inbox: { layout: "repository" } } });
  await settingSaved(request, (s) => s.inbox.layout, "repository");
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
    await loose(page).getByTestId("project-repo").filter({ hasText: repo }).dragTo(card(page, "Proj"));
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
  await seed.getByRole("button", { name: "Move proj/seed" }).click();
  await page.getByRole("menuitem", { name: "New project…" }).click();
  const dialog = page.getByRole("dialog", { name: "New project" });
  await expect(dialog).toContainText("It starts with proj/seed");
  await dialog.getByLabel("Name", { exact: true }).fill("Seeded");
  await dialog.getByRole("button", { name: "Create project" }).click();
  await expect(card(page, "Seeded").getByTestId("project-repo")).toHaveText([/proj\/seed/]);
});

test("a repository moves between projects by dragging and from its menu", async ({ page, request }) => {
  await createReview(request, { title: "Projects: web", origin: { repo: "proj/web" } });
  for (const name of ["One", "Two"]) {
    const response = await request.post(`${core}/api/v1/projects`, { data: { name } });
    expect(response.status()).toBe(201);
  }
  await page.goto("/#/projects");
  const web = loose(page).getByTestId("project-repo").filter({ hasText: "proj/web" });

  // dragged onto a card, it goes in that project
  await web.dragTo(card(page, "One"));
  await expect(card(page, "One")).toContainText("proj/web");
  await expect(loose(page).getByText("proj/web")).toHaveCount(0);

  // its menu moves it to another, and takes it out of every project
  const inOne = card(page, "One").getByTestId("project-repo").filter({ hasText: "proj/web" });
  await inOne.hover();
  await inOne.getByRole("button", { name: "Move proj/web" }).click();
  await page.getByRole("menuitem", { name: "Two" }).click();
  await expect(card(page, "Two")).toContainText("proj/web");
  await expect(card(page, "One")).not.toContainText("proj/web");

  const inTwo = card(page, "Two").getByTestId("project-repo").filter({ hasText: "proj/web" });
  await inTwo.hover();
  await inTwo.getByRole("button", { name: "Move proj/web" }).click();
  await page.getByRole("menuitem", { name: "Remove from Two" }).click();
  await expect(loose(page)).toContainText("proj/web");

  // a repository nothing has sent from yet is added by its name
  await card(page, "One").getByRole("button", { name: "Add a repository to One" }).click();
  await card(page, "One").getByLabel("Repository", { exact: true }).fill("proj/later");
  await card(page, "One").getByLabel("Repository", { exact: true }).press("Enter");
  await expect(card(page, "One")).toContainText("proj/later");
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
  await loose(page).getByTestId("project-repo").filter({ hasText: "keep/c" }).dragTo(card(page, "quiet"));
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
  await page.getByRole("radio", { name: "Grouped by project" }).click();
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
  await page.getByRole("button", { name: "Project or repository" }).click();
  await page.getByRole("option", { name: "Mix", exact: true }).click();
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
  const filter = page.getByRole("button", { name: "Project or repository" });

  // typing on the closed filter opens it with a search for what was typed
  await filter.focus();
  await page.keyboard.type("we");
  const search = page.getByRole("textbox", { name: "Filter project or repository" });
  await expect(search).toBeFocused();
  await expect(search).toHaveValue("we");
  await expect(page.getByRole("option").locator(".select-option-label")).toHaveText(["typed/web"]);

  // the projects match by their names too; the arrow keys and Enter pick a match
  await search.fill("TYPED");
  await expect(page.getByRole("option").locator(".select-option-label")).toHaveText([
    "Typed",
    "typed/app",
    "typed/web",
  ]);
  await search.press("ArrowDown");
  await search.press("Enter");
  await expect(page).toHaveURL(/repo=typed%2Fapp/);
  await expect(page.getByTestId("review-row")).toHaveText([/Type: app/]);

  // nothing matching says so, and Escape leaves the choice as it was
  await filter.click();
  await expect(search).toHaveValue("");
  await search.fill("nothing like it");
  await expect(page.getByRole("listbox")).toContainText("No matches");
  await search.press("Escape");
  await expect(page.getByRole("listbox")).toHaveCount(0);
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
  await edit.getByRole("radio", { name: "Green" }).click();
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

  // the inbox's group shows it too
  await page.goto("/#/");
  await page.getByRole("radio", { name: "Grouped by project" }).click();
  await expect(
    page.getByTestId("inbox-group").filter({ hasText: "Avatar" }).locator(".project-avatar"),
  ).toHaveAttribute("data-color", "green");
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
