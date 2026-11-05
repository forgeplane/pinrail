// The Projects page: projects made and changed in the app, listed by name,
// each with its repositories, and the repositories in no project last.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { core, createReview } from "./helpers";

/** Deletes every project, so each test starts with none. */
async function noProjects(request: APIRequestContext) {
  const { projects } = (await (await request.get(`${core}/api/v1/projects`)).json()) as { projects: { id: string }[] };
  for (const p of projects) await request.delete(`${core}/api/v1/projects/${p.id}`);
}

const card = (page: Page, name: string) => page.getByRole("region", { name, exact: true });
const loose = (page: Page) => page.getByRole("region", { name: "Repositories in no project" });

test.beforeEach(({ request }) => noProjects(request));
test.afterAll(({ request }) => noProjects(request));

test("a project is created with repositories, renamed and deleted", async ({ page }) => {
  await createReview(page.request, { title: "Projects: app", origin: { repo: "proj/app" } });
  await createReview(page.request, { title: "Projects: infra", origin: { repo: "proj/infra" } });

  await page.goto("/#/projects");
  await expect(page.getByRole("heading", { name: "Group repositories into projects" })).toBeVisible();
  await expect(loose(page)).toContainText("proj/app");
  await page.getByRole("button", { name: "New project" }).click();

  const dialog = page.getByRole("dialog", { name: "New project" });
  await dialog.getByLabel("Name").fill("Proj");
  await dialog.getByRole("checkbox", { name: "proj/app" }).check();
  await dialog.getByRole("checkbox", { name: "proj/infra" }).check();
  await dialog.getByRole("button", { name: "Create project" }).click();
  await expect(dialog).toBeHidden();

  await expect(card(page, "Proj").getByTestId("project-repo")).toHaveText([/proj\/app/, /proj\/infra/]);
  await expect(card(page, "Proj")).toContainText("2 waiting");
  await expect(loose(page).getByText("proj/app")).toHaveCount(0);

  // a name another project has is refused, whatever its case
  await page.getByRole("button", { name: "New project" }).click();
  await dialog.getByLabel("Name").fill("proj");
  await dialog.getByRole("button", { name: "Create project" }).click();
  await expect(dialog).toContainText("another project has this name");
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();

  await card(page, "Proj").getByRole("button", { name: "Proj actions" }).click();
  await page.getByRole("menuitem", { name: "Rename…" }).click();
  const rename = page.getByRole("dialog", { name: "Rename project" });
  await rename.getByLabel("Name").fill("Proj app");
  await rename.getByRole("button", { name: "Rename" }).click();
  await expect(card(page, "Proj app")).toBeVisible();

  await card(page, "Proj app").getByRole("button", { name: "Proj app actions" }).click();
  await page.getByRole("menuitem", { name: "Delete project…" }).click();
  await page.getByRole("dialog", { name: "Delete Proj app?" }).getByRole("button", { name: "Delete project" }).click();
  await expect(page.getByTestId("project-group")).toHaveCount(0);
  // its repositories are in no project again
  await expect(loose(page)).toContainText("proj/infra");
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
