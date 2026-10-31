import { expect, test, type APIRequestContext, type Locator, type Page } from "@playwright/test";
import { clearInbox, core, createReview, decide, settingsDialog, onReviewPages } from "./helpers";

// these tests open a review from the inbox or the sidebar on a page of its own
onReviewPages();

const payload = {
  summary: "Two proposals.",
  groups: [
    {
      title: "lib/acme/tickets.ex",
      items: [
        { id: 1, severity: "major", title: "do_save dedups without reversing" },
        { id: 2, severity: "minor", title: "moduledoc typo" },
      ],
    },
  ],
};

/** A list review from `workflow`, in acme/api unless told otherwise. */
function review(request: APIRequestContext, title: string, workflow: string, repo: string | null = "acme/api") {
  return createReview(request, { title, origin: repo ? { repo, workflow } : { workflow }, payload });
}

/** Leaves both proposals undecided, which decides the review. */
const decideNothing = (request: APIRequestContext, id: string) =>
  decide(request, id, { decisions: [], undecided: [1, 2] });

/** A project's group in the sidebar: its name, its count, and what waits in it. */
const project = (page: Page, name: string) =>
  page
    .getByRole("region", { name: "Repositories" })
    .getByTestId("sidebar-group")
    .filter({ has: page.getByTestId("sidebar-group-name").getByText(name, { exact: true }) });

test("the sidebar lists what waits under its project on every page, and ⌥↓ walks it", async ({ page }) => {
  await clearInbox(page.request);
  const first = await review(page.request, "Sidebar: the older one", "pr-review");
  const second = await review(page.request, "Sidebar: the newer one", "triage");

  const loose = await review(page.request, "Sidebar: no project", "cron", null);

  // on the history tab, not the waiting list: what waits is listed under
  // its project with the project's count, and a review that names no
  // project has a group of its own
  await page.goto("/#/?tab=history");
  const projects = page.getByRole("region", { name: "Repositories" });
  await expect(project(page, "acme/api")).toContainText("2");
  await expect(project(page, "No repository")).toContainText("1");
  await project(page, "No repository").getByRole("link", { name: "No repository" }).click();
  await expect(page).toHaveURL(/repo=-/);
  await expect(page.getByTestId("inbox-group-name")).toHaveText(["No project"]);
  await expect(page.locator("#inbox-repo")).toContainText("No project");
  await decideNothing(page.request, loose.id);
  await expect(projects.getByTestId("waiting-review").filter({ hasText: "no project" })).toHaveCount(0);
  await page.goto("/#/?tab=history");
  const rows = projects.getByTestId("waiting-review");
  // newest first, as the inbox lists them
  await expect(rows).toHaveText([/the newer one/, /the older one/]);

  // a click opens the review and the row reads as current
  await rows.nth(0).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${second.id}$`));
  await expect(rows.nth(0)).toHaveAttribute("aria-current", "page");

  // ⌥↓ moves to the next waiting review, oldest first, wrapping; ⌥↑ goes
  // back. The keys are the shell's, so the view's frame must not hold the focus.
  await page.keyboard.press("Alt+ArrowDown");
  await expect(page).toHaveURL(new RegExp(`/reviews/${first.id}$`));
  await page.getByTestId("top-bar").click();
  await page.keyboard.press("Alt+ArrowUp");
  await expect(page).toHaveURL(new RegExp(`/reviews/${second.id}$`));

  // still there inside settings
  await page.keyboard.press("ControlOrMeta+,");
  await expect(settingsDialog(page)).toBeVisible();
  await expect(projects).toBeVisible();
  await page.keyboard.press("Escape");

  // a decision takes its row away
  await decideNothing(page.request, second.id);
  await expect(rows).toHaveText([/the older one/]);
  await decideNothing(page.request, first.id);
  await expect(rows).toHaveCount(0);
});

test("a window too narrow for the sidebar gives its whole width to the page", async ({ page }) => {
  await page.setViewportSize({ width: 740, height: 600 });
  await page.goto("/#/history");
  await expect(page.getByRole("complementary", { name: "Workspace" })).toBeHidden();
  const shell = await page.getByRole("main").boundingBox();
  expect(shell?.x).toBe(0);
  expect(shell?.width).toBe(740);
});

test("⌘B hides the sidebar and shows it again", async ({ page }) => {
  await page.goto("/#/history");
  const toggle = page.getByRole("button", { name: /^(Hide|Show) sidebar$/ });
  await expect(toggle).toHaveAttribute("aria-label", "Hide sidebar");

  await page.keyboard.press("ControlOrMeta+b");
  await expect(toggle).toHaveAttribute("aria-label", "Show sidebar");
  await page.keyboard.press("ControlOrMeta+b");
  await expect(toggle).toHaveAttribute("aria-label", "Hide sidebar");
});

test("a quick second ⌘B is not undone by the reload the first one caused", async ({ page }) => {
  await page.goto("/#/history");
  const toggle = page.getByRole("button", { name: /^(Hide|Show) sidebar$/ });
  await expect(toggle).toHaveAttribute("aria-label", "Hide sidebar");
  // a slow server: the settings the first change makes the app reload come
  // back after the second change
  await page.route(`${core}/api/v1/settings`, async (route) => {
    if (route.request().method() !== "GET") return route.continue();
    const response = await route.fetch();
    await new Promise((r) => setTimeout(r, 800));
    await route.fulfill({ response });
  });

  await page.keyboard.press("ControlOrMeta+b");
  await expect(toggle).toHaveAttribute("aria-label", "Show sidebar");
  await page.waitForTimeout(200);
  await page.keyboard.press("ControlOrMeta+b");
  await expect(toggle).toHaveAttribute("aria-label", "Hide sidebar");
  // and it stays shown while every reload comes back
  const seen = new Set<string | null>();
  for (let i = 0; i < 40; i++) {
    seen.add(await toggle.getAttribute("aria-label"));
    await page.waitForTimeout(50);
  }
  expect([...seen]).toEqual(["Hide sidebar"]);
  await page.unroute(`${core}/api/v1/settings`);
});

test("the app's own text cannot be selected, as in a desktop app, but fields and messages can", async ({ page }) => {
  await page.goto("/");
  const selectable = (locator: Locator) =>
    locator.first().evaluate((el) => getComputedStyle(el).webkitUserSelect || getComputedStyle(el).userSelect);
  await expect(page.getByTestId("inbox-count")).toBeVisible();
  expect(await selectable(page.getByTestId("inbox-count"))).toBe("none");
  expect(await selectable(page.locator("h1"))).toBe("none");
  expect(await selectable(page.locator("input[type=search]"))).not.toBe("none");

  // a review's title and id are there to be copied
  const review = await createReview(page.request, { title: "Copy my title", payload });
  await page.goto(`/#/reviews/${review.id}`);
  await expect(page.getByTestId("review-title")).toHaveText("Copy my title");
  expect(await selectable(page.getByTestId("review-title"))).not.toBe("none");
  expect(await selectable(page.getByTestId("review-id"))).not.toBe("none");
});

test("the sidebar's edge resizes it, within its bounds, and the width stays", async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 700 });
  await page.goto("/#/");
  await page.evaluate(() => localStorage.removeItem("pinrail.sidebar.width"));
  await page.reload();
  const sidebar = page.getByRole("complementary", { name: "Workspace" });
  const edge = page.getByRole("separator", { name: "Resize the sidebar" });
  const width = async () => Math.round((await sidebar.boundingBox())!.width);
  expect(await width()).toBe(220);

  const drag = async (x: number) => {
    const box = (await edge.boundingBox())!;
    await page.mouse.move(box.x + box.width / 2, box.y + 200);
    await page.mouse.down();
    await page.mouse.move(x, box.y + 200, { steps: 5 });
    await page.mouse.up();
  };
  await drag(320);
  expect(await width()).toBe(320);
  // not narrower than its minimum, nor wider than its maximum
  await drag(50);
  expect(await width()).toBe(200);
  await drag(900);
  expect(await width()).toBe(420);

  // the arrow keys on the edge, and the width after a reload
  await edge.focus();
  await page.keyboard.press("ArrowLeft");
  expect(await width()).toBe(400);
  await page.reload();
  expect(await width()).toBe(400);
  // a double click puts it back
  await edge.dblclick();
  expect(await width()).toBe(220);
});
