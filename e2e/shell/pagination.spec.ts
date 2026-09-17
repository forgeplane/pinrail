import { expect, test, type APIRequestContext } from "@playwright/test";

const core = "http://127.0.0.1:4799";

const payload = {
  intro: "One proposal.",
  allow_additions: false,
  groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }],
};

async function createReview(request: APIRequestContext, title: string, repo: string) {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: { plugin: "list", title, origin: { repo, workflow: "paging" }, requested_by: "spec", payload },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { id: string };
}

/** Discards whatever is pending, so a test starts from an empty inbox. */
async function clearInbox(request: APIRequestContext) {
  const pending = (await (await request.get(`${core}/api/v1/reviews?status=pending&limit=500`)).json()).reviews as { id: string }[];
  for (const r of pending) await request.post(`${core}/api/v1/reviews/${r.id}/discard`, { data: { reason: "spec cleanup" } });
}

test("history shows 50 a page, pages through the rest, and starts again at page 1 when filtered", async ({ page }) => {
  await clearInbox(page.request);
  // 60 decided reviews under a word no other spec uses, oldest first
  for (let i = 1; i <= 60; i++) {
    const { id } = await createReview(page.request, `Paging history ${String(i).padStart(2, "0")}`, i % 2 ? "acme/odd" : "acme/even");
    const decided = await page.request.post(`${core}/api/v1/reviews/${id}/decision`, { data: { data: { decisions: [], undecided: [1] } } });
    expect(decided.status()).toBe(200);
  }

  await page.goto("/#/history?q=paging");
  const rows = page.locator("[data-history-row]");
  const range = page.locator("[data-pager-range]");
  await expect(rows).toHaveCount(50);
  await expect(range).toHaveText("1–50 of 60");
  await expect(rows.first()).toContainText("Paging history 60");
  await expect(page.locator("[data-pager-previous]")).toBeDisabled();

  await page.locator("[data-pager-next]").click();
  await expect(page).toHaveURL(/page=2/);
  await expect(rows).toHaveCount(10);
  await expect(range).toHaveText("51–60 of 60");
  await expect(rows.last()).toContainText("Paging history 01");
  await expect(page.locator("[data-pager-next]")).toBeDisabled();

  // a filter narrows on the server and goes back to the first page
  await page.getByRole("searchbox", { name: "Search history" }).fill("paging acme/odd");
  await expect(page).not.toHaveURL(/page=/);
  await expect(rows).toHaveCount(30);
  await expect(range).toHaveText("1–30 of 30");

  // the page size: 25 splits the 30 in two
  await page.goto("/#/history?q=paging%20acme/odd&per=25");
  await expect(rows).toHaveCount(25);
  await expect(range).toHaveText("1–25 of 30");
  await page.locator("[data-pager-next]").click();
  await expect(rows).toHaveCount(5);
  await expect(range).toHaveText("26–30 of 30");

  // the search reads the requester and the project as well as the title
  await page.goto("/#/history?q=acme/even%20spec");
  await expect(rows).toHaveCount(30);
});

test("the inbox holds every pending review, 50 a page, and Enter opens the row that is lit", async ({ page }) => {
  await clearInbox(page.request);
  // 105: past the 100 the inbox used to stop at, across two projects
  for (let i = 1; i <= 105; i++) {
    await createReview(page.request, `Paging inbox ${String(i).padStart(3, "0")}`, i % 3 ? "acme/api" : "acme/web");
  }

  await page.goto("/#/");
  const rows = page.locator("[data-review-row]");
  const range = page.locator("[data-pager-range]");
  await expect(rows).toHaveCount(50);
  await expect(range).toHaveText("1–50 of 105");

  // the project headers count the whole project, not the page
  await expect(page.locator(".inbox-repo summary").filter({ hasText: "acme/api" })).toContainText("70");

  await page.locator("[data-pager-next]").click();
  await expect(rows).toHaveCount(50);
  await expect(range).toHaveText("51–100 of 105");
  await page.locator("[data-pager-next]").click();
  await expect(rows).toHaveCount(5);
  await expect(range).toHaveText("101–105 of 105");

  // J moves down the rows as drawn, and Enter opens the one that is lit
  await page.locator("[data-pager-previous]").click();
  await page.locator("[data-pager-previous]").click();
  await expect(range).toHaveText("1–50 of 105");
  // the pointer out of the way: a row under it would take the focus
  await page.mouse.move(0, 0);
  await page.locator("h1").click();
  for (let i = 0; i < 3; i++) await page.keyboard.press("j");
  const lit = page.locator("[data-review-row].is-focused");
  await expect(lit).toHaveCount(1);
  const href = await lit.getAttribute("href");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(new RegExp(`${href!.replace(/^#/, "")}$`));
});
