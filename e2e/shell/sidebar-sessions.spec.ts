// The sessions in the sidebar: grouped under their projects, with a mark for
// a session that works or is done, what waits under the session that sent
// it, and the heading's menu, whose choices the settings keep.

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { clearInbox, core, onePayload, settingSaved } from "./helpers";

let n = 0;
/** A project and sessions of each test's own, so other tests' sessions stay apart. */
const fresh = () => `${Date.now()}-${++n}`;

const DEFAULTS = {
  grouping: "repository",
  ordering: "status",
  show_details: false,
  details: ["activity", "status"],
  show_reviews: true,
  agents: [],
  repos: [],
  states: [],
  archived: false,
  collapsed: [],
};
const setChoices = (request: APIRequestContext, sessions: Record<string, unknown>) =>
  request.patch(`${core}/api/v1/settings`, { data: { sidebar: { sessions } } });

test.afterEach(({ request }) => setChoices(request, DEFAULTS));

async function report(request: APIRequestContext, agent: string, session: string, body: Record<string, unknown>) {
  const response = await request.post(`${core}/api/v1/sessions/${agent}/${session}/status`, { data: body });
  expect(response.status(), await response.text()).toBe(200);
}
async function ask(request: APIRequestContext, agent: string, session: string, repo: string, title: string) {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: {
      kind: "question",
      title,
      origin: { repo },
      requested_by: agent,
      session,
      payload: { questions: [{ id: "q1", question: "Go ahead?", answer: "yes-no" }] },
    },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { id: string };
}

const region = (page: Page) => page.getByRole("complementary", { name: "Workspace" });
const group = (page: Page, name: string) =>
  region(page)
    .getByTestId("sidebar-group")
    .filter({ has: page.getByTestId("sidebar-group-name").getByText(name, { exact: true }) });
const sessionRow = (page: Page, agent: string, session: string) =>
  region(page)
    .getByTestId("sidebar-session")
    .and(page.locator(`[href="#/sessions/${agent}/${session}"]`));
const openMenu = async (page: Page) => {
  await region(page).getByRole("button", { name: "Sessions options" }).click();
  return page.getByTestId("sidebar-sessions-menu");
};

test("a session shows whether it works or is done, and what waits sits under it", async ({ page }) => {
  await clearInbox(page.request);
  const run = fresh();
  const repo = `side/${run}`;
  const busy = `busy-${run}`;
  const finished = `done-${run}`;
  await report(page.request, "claude-code", busy, { text: "Build the sidebar", kind: "start", origin: { repo } });
  await report(page.request, "claude-code", busy, { text: "Running the tests" });
  await ask(page.request, "claude-code", busy, repo, `Sidebar: may I push? ${run}`);
  await report(page.request, "codex", finished, { text: "Fix the lint", kind: "start", origin: { repo } });
  await report(page.request, "codex", finished, { text: "Pushed", kind: "done" });

  await page.goto("/#/");
  const working = sessionRow(page, "claude-code", busy);
  const done = sessionRow(page, "codex", finished);
  await expect(working).toContainText("Build the sidebar");
  await expect(working).toHaveAttribute("data-state", "working");
  await expect(done).toHaveAttribute("data-state", "done");
  // the project counts what waits in it, and the session too
  await expect(group(page, repo).locator(".nav-count")).toHaveText("1");
  await expect(working.getByTestId("sidebar-session-waiting")).toHaveText("1");
  // what waits first, the done one last
  await expect(
    region(page)
      .getByTestId("sidebar-session")
      .filter({ hasText: /Build the sidebar|Fix the lint/ }),
  ).toHaveText([/Build the sidebar/, /Fix the lint/]);

  // the question under its session opens where it is answered: its session's page
  const question = region(page)
    .getByTestId("waiting-review")
    .filter({ hasText: `may I push? ${run}` });
  await expect(question).toHaveClass(/is-question/);
  await question.click();
  await expect(page).toHaveURL(new RegExp(`#/sessions/claude-code/${busy}$`));
  await expect(page.getByTestId("question").filter({ hasText: "Go ahead?" })).toBeVisible();

  // a review opens on its own page, whatever the inbox opens reviews in
  const sent = await page.request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "list",
      title: `Sidebar: the plan ${run}`,
      origin: { repo },
      requested_by: "claude-code",
      session: busy,
      payload: onePayload,
    },
  });
  const plan = (await sent.json()) as { id: string };
  await region(page)
    .getByTestId("waiting-review")
    .filter({ hasText: `the plan ${run}` })
    .click();
  await expect(page).toHaveURL(new RegExp(`#/reviews/${plan.id}$`));

  // the tooltip says what the mark means and what the agent last said
  await working.hover();
  await expect(page.getByRole("tooltip")).toContainText("Working");
  await expect(page.getByRole("tooltip")).toContainText("Running the tests");

  // archiving takes a session out of the sidebar
  expect((await page.request.put(`${core}/api/v1/sessions/codex/${finished}/archive`)).status()).toBe(204);
  await expect(done).toHaveCount(0);
});

test("the heading's menu groups, shows and filters the sessions, and the settings keep it", async ({ page }) => {
  await clearInbox(page.request);
  const run = fresh();
  const repo = `side/${run}`;
  const mine = `menu-${run}`;
  const theirs = `menu-codex-${run}`;
  await report(page.request, "claude-code", mine, { text: "Draw the menu", kind: "start", origin: { repo } });
  await ask(page.request, "claude-code", mine, repo, `Sidebar: the menu? ${run}`);
  await report(page.request, "codex", theirs, { text: "Tidy up", kind: "start", origin: { repo, ref: "tidy-up" } });

  await page.goto("/#/");
  const heading = region(page)
    .getByRole("heading", { level: 2 })
    .filter({ has: page.getByRole("button", { name: "Sessions options" }) });
  await expect(heading).toHaveText("Repositories");

  // grouping by status names the heading after it, and survives a reload
  let menu = await openMenu(page);
  await menu.getByRole("menuitem", { name: /Grouping/ }).click();
  // each grouping has its icon
  await expect(menu.getByRole("menuitemradio").locator(".menu-icon")).toHaveCount(5);
  await menu.getByRole("menuitemradio", { name: "Status" }).click();
  await expect(heading).toHaveText("Status");
  await expect(group(page, "Waiting for you")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await page.reload();
  await expect(heading).toHaveText("Status");

  // the reviews under their session can be hidden
  const question = region(page)
    .getByTestId("waiting-review")
    .filter({ hasText: `the menu? ${run}` });
  await expect(question).toBeVisible();
  menu = await openMenu(page);
  await menu.getByRole("menuitemcheckbox", { name: "Show reviews" }).click();
  await expect(question).toHaveCount(0);

  // a second line of details
  await menu.getByRole("menuitemcheckbox", { name: "Show details" }).click();
  await expect(sessionRow(page, "codex", theirs).locator(".sidebar-session-details")).toContainText("ago");

  // the branch, with its icon
  await menu.getByRole("menuitem", { name: /Details/ }).click();
  await menu.getByRole("menuitemcheckbox", { name: "Branch" }).click();
  const branch = sessionRow(page, "codex", theirs).locator(".sidebar-session-branch");
  await expect(branch).toHaveText("tidy-up");
  await expect(branch.locator("svg")).toBeVisible();

  // an agent filter, and Reset
  await menu.getByRole("menuitem", { name: /Agent/ }).click();
  await menu.getByRole("menuitemcheckbox", { name: "codex" }).click();
  await expect(sessionRow(page, "claude-code", mine)).toHaveCount(0);
  await expect(sessionRow(page, "codex", theirs)).toBeVisible();
  await menu.getByRole("button", { name: "Reset" }).click();
  await expect(sessionRow(page, "claude-code", mine)).toBeVisible();
});

test("grouped by project, a project's repositories share a group, and each session names its own", async ({ page }) => {
  await clearInbox(page.request);
  const run = fresh();
  const [app, infra, other] = [`proj-app/${run}`, `proj-infra/${run}`, `proj-other/${run}`];
  await report(page.request, "claude-code", `p-app-${run}`, { text: "App work", kind: "start", origin: { repo: app } });
  await report(page.request, "claude-code", `p-infra-${run}`, {
    text: "Infra work",
    kind: "start",
    origin: { repo: infra },
  });
  await report(page.request, "codex", `p-other-${run}`, { text: "Other work", kind: "start", origin: { repo: other } });
  const name = `Proj ${run}`;
  const created = await page.request.post(`${core}/api/v1/projects`, { data: { name, repos: [app, infra] } });
  expect(created.status()).toBe(201);
  const project = (await created.json()) as { id: string };

  try {
    await setChoices(page.request, { grouping: "project" });
    await page.goto("/#/");
    const heading = region(page)
      .getByRole("heading", { level: 2 })
      .filter({ has: page.getByRole("button", { name: "Sessions options" }) });
    await expect(heading).toHaveText("Projects");
    await expect(group(page, name)).toBeVisible();
    // its avatar's letter stays white, whatever colour the row gives its icons
    await expect(group(page, name).locator(".project-avatar text")).toHaveCSS("fill", "rgb(255, 255, 255)");
    // the repository in no project is a group of its own
    await expect(group(page, other)).toBeVisible();
    await expect(sessionRow(page, "claude-code", `p-app-${run}`).locator(".sidebar-session-repo-label")).toHaveText(
      run,
    );

    // the filter takes the project, and keeps its repositories' sessions
    const menu = await openMenu(page);
    await menu.getByRole("menuitem", { name: /Project or repository/ }).click();
    // as in the inbox's filter: typed words narrow the list, and a project
    // shows its avatar
    const submenu = page.getByRole("menu", { name: "Project or repository" });
    await submenu.getByRole("textbox", { name: "Filter project or repository" }).fill(`proj ${run}`);
    await expect(submenu.getByRole("menuitemcheckbox")).toHaveCount(1);
    await expect(submenu.getByRole("menuitemcheckbox", { name }).locator(".project-avatar")).toBeVisible();
    await menu.getByRole("menuitemcheckbox", { name }).click();
    await expect(sessionRow(page, "claude-code", `p-infra-${run}`)).toBeVisible();
    await expect(sessionRow(page, "codex", `p-other-${run}`)).toHaveCount(0);
    await settingSaved(page.request, (s) => s.sidebar.sessions.repos, [`project:${project.id}`]);
  } finally {
    await page.request.delete(`${core}/api/v1/projects/${project.id}`);
  }
});

test("a group collapses and stays collapsed, and the menu collapses them all", async ({ page }) => {
  await clearInbox(page.request);
  const run = fresh();
  const repo = `side/${run}`;
  const session = `fold-${run}`;
  await report(page.request, "claude-code", session, { text: "Fold the groups", kind: "start", origin: { repo } });

  await page.goto("/#/");
  const row = sessionRow(page, "claude-code", session);
  await expect(row).toBeVisible();
  await group(page, repo)
    .getByRole("button", { name: `Collapse ${repo}` })
    .click();
  await expect(row).toHaveCount(0);
  await page.reload();
  await expect(group(page, repo)).toBeVisible();
  await expect(row).toHaveCount(0);
  await group(page, repo)
    .getByRole("button", { name: `Expand ${repo}` })
    .click();
  await expect(row).toBeVisible();

  const menu = await openMenu(page);
  await menu.getByRole("menuitem", { name: "Collapse all groups" }).click();
  await expect(region(page).getByTestId("sidebar-session")).toHaveCount(0);
  await (await openMenu(page)).getByRole("menuitem", { name: "Expand all groups" }).click();
  await expect(row).toBeVisible();
});

test("a question under its session opens its thread, and the session's row keeps it", async ({ page }) => {
  await clearInbox(page.request);
  const run = fresh();
  const repo = `side/${run}`;
  const session = `thread-${run}`;
  await report(page.request, "claude-code", session, { text: "Keep the thread", kind: "start", origin: { repo } });
  await ask(page.request, "claude-code", session, repo, `Sidebar: same thread? ${run}`);

  await page.goto("/#/?tab=history");
  await region(page)
    .getByTestId("waiting-review")
    .filter({ hasText: `same thread? ${run}` })
    .click();
  const thread = page.getByTestId("thread");
  await expect(thread.getByTestId("question")).toBeVisible();
  await thread.evaluate((el) => ((window as unknown as { drawn: Element }).drawn = el));
  await sessionRow(page, "claude-code", session).click();
  await expect(page).toHaveURL(new RegExp(`#/sessions/claude-code/${session}$`));
  // after the session's page had its chance to draw: the same thread, not a new one
  await page.waitForTimeout(500);
  expect(await page.evaluate(() => document.contains((window as unknown as { drawn: Element }).drawn))).toBe(true);
});

test("a long branch in a session's details shortens, and keeps its icon and its start", async ({ page }) => {
  await clearInbox(page.request);
  const run = fresh();
  const repo = `side/${run}`;
  const session = `branch-${run}`;
  const ref = "feature/a-branch-name-far-too-long-for-the-sidebar";
  await report(page.request, "claude-code", session, { text: "Long branch", kind: "start", origin: { repo, ref } });
  await setChoices(page.request, { show_details: true, details: ["activity", "branch"] });
  await page.setViewportSize({ width: 1200, height: 700 });
  await page.goto("/#/");
  await page.evaluate(() => localStorage.removeItem("pinrail.sidebar.width"));
  await page.reload();
  const branch = sessionRow(page, "claude-code", session).locator(".sidebar-session-branch");
  await expect(branch.locator("svg")).toBeInViewport();
  // the name keeps its first letters in view, shortened at its end
  const name = branch.locator(".sidebar-session-branch-name");
  await expect(name).toHaveText(ref);
  expect((await name.boundingBox())?.width ?? 0).toBeGreaterThan(30);
});

test("a session's details can show its repository, except when grouped by repository", async ({ page }) => {
  await clearInbox(page.request);
  const run = fresh();
  const repo = `side/${run}`;
  const session = `repo-${run}`;
  await report(page.request, "claude-code", session, {
    text: "Show the repository",
    kind: "start",
    origin: { repo, ref: "main" },
  });
  await setChoices(page.request, { grouping: "status", show_details: true, details: ["activity", "repo"] });
  await page.goto("/#/");
  const shown = sessionRow(page, "claude-code", session).locator(".sidebar-session-repo");
  await expect(shown).toHaveText(repo);
  await expect(shown.locator("svg")).toBeVisible();
  // the repository comes before the branch, whichever was chosen first
  await setChoices(page.request, { details: ["branch", "repo"] });
  await page.reload();
  const parts = sessionRow(page, "claude-code", session).locator(".sidebar-session-details > :not([aria-hidden])");
  await expect(parts).toHaveText([repo, "main"]);

  // grouped by repository, the group names it already
  const menu = await openMenu(page);
  await menu.getByRole("menuitem", { name: /Grouping/ }).click();
  await menu.getByRole("menuitemradio", { name: "Repository" }).click();
  await expect(shown).toHaveCount(0);
  await menu.getByRole("menuitem", { name: /Details/ }).click();
  await expect(menu.getByRole("menuitemcheckbox", { name: /Repository/ })).toBeDisabled();
  await settingSaved(page.request, (s) => s.sidebar.sessions.grouping, "repository");
});

test("what waits under a session lines up with the session's title, grouped or not", async ({ page }) => {
  await clearInbox(page.request);
  const run = fresh();
  const session = `child-${run}`;
  await report(page.request, "claude-code", session, {
    text: "Line up",
    kind: "start",
    origin: { repo: `side/${run}` },
  });
  const ask = await page.request.post(`${core}/api/v1/reviews`, {
    data: {
      kind: "question",
      title: "Lined up?",
      requested_by: "claude-code",
      session,
      origin: { repo: `side/${run}` },
      payload: { questions: [{ id: "q1", question: "Lined up?", answer: "yes-no" }] },
    },
  });
  expect(ask.status()).toBe(201);
  for (const grouping of ["repository", "none"]) {
    await setChoices(page.request, { grouping });
    await page.goto("/#/?tab=history");
    const row = sessionRow(page, "claude-code", session);
    const title = await row.locator(".sidebar-review-title").boundingBox();
    const child = page.getByTestId("waiting-review").filter({ hasText: "Lined up?" });
    const icon = await child.locator("svg").first().boundingBox();
    expect(Math.abs((icon?.x ?? 0) - (title?.x ?? 0)), grouping).toBeLessThanOrEqual(2);
  }
});

test("a submenu near the window's bottom moves up to fit in it", async ({ page }) => {
  const run = fresh();
  for (let i = 0; i < 8; i++) {
    await report(page.request, "claude-code", `fit-${i}-${run}`, {
      text: "Fit",
      kind: "start",
      origin: { repo: `fit/${i}-${run}` },
    });
  }
  await page.setViewportSize({ width: 1000, height: 560 });
  await page.goto("/#/");
  const menu = await openMenu(page);
  await menu.getByRole("menuitem", { name: /Project or repository/ }).click();
  await expect(page.getByRole("menu", { name: "Project or repository" })).toBeInViewport({ ratio: 1 });
});

test("a change made elsewhere while the app saves its own is shown once the save is answered", async ({ page }) => {
  await setChoices(page.request, { ...DEFAULTS, agents: ["codex"] });
  await page.goto("/#/");
  const clear = page.locator(".sidebar-sessions > h2").getByRole("button", { name: "Clear the sidebar's filters" });
  await expect(clear).toBeVisible();

  // the core stores the app's save of the cleared filters, and its answer is held
  let release!: () => void;
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route(`${core}/api/v1/settings`, async (route) => {
    if (route.request().method() !== "PATCH") return route.fallback();
    const response = await route.fetch();
    await held;
    await route.fulfill({ response });
  });
  const saved = page.waitForResponse((r) => r.url() === `${core}/api/v1/settings` && r.request().method() === "PATCH");
  await clear.click();
  await expect(clear).toHaveCount(0);
  await settingSaved(page.request, (s) => s.sidebar.sessions.agents.length, 0);

  // meanwhile the CLI or another window shows the archived sessions; its
  // notice has time to arrive while the save waits
  await setChoices(page.request, { ...DEFAULTS, archived: true });
  await page.waitForTimeout(500);
  release();
  await saved;
  await expect(clear).toBeVisible();
});

test("a Clear filters button shows in the heading while a filter is on, and clears them", async ({ page }) => {
  await page.goto("/#/");
  // the heading names what the sessions are grouped by
  const heading = page.locator(".sidebar-sessions > h2");
  const clear = heading.getByRole("button", { name: "Clear the sidebar's filters" });
  await expect(heading).toBeVisible();
  await expect(clear).toHaveCount(0);

  for (const filter of [{ agents: ["codex"] }, { states: ["working"] }, { archived: true }]) {
    await setChoices(page.request, { ...DEFAULTS, ...filter });
    await expect(clear).toBeVisible();
    await clear.click();
    await expect(clear).toHaveCount(0);
    await settingSaved(page.request, (s) => s.sidebar.sessions.agents.length + s.sidebar.sessions.states.length, 0);
    await settingSaved(page.request, (s) => s.sidebar.sessions.archived, false);
  }
  // the grouping and the details are not filters, and stay as they are
  await setChoices(page.request, { ...DEFAULTS, grouping: "status", agents: ["codex"] });
  await clear.click();
  await settingSaved(page.request, (s) => s.sidebar.sessions.agents.length, 0);
  await settingSaved(page.request, (s) => s.sidebar.sessions.grouping, "status");
  // beside the options button, at the heading's end
  await setChoices(page.request, { ...DEFAULTS, agents: ["codex"] });
  await expect(clear).toBeVisible();
  const options = heading.getByRole("button", { name: "Sessions options" });
  const [a, b] = [await clear.boundingBox(), await options.boundingBox()];
  expect(b!.x - (a!.x + a!.width)).toBeLessThan(6);
});
