import { expect, test, type APIRequestContext } from "@playwright/test";

const core = "http://127.0.0.1:4799";

const payload = {
  intro: "One proposal.",
  groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, severity: "minor", title: "moduledoc typo" }] }],
};

async function createReview(request: APIRequestContext, title: string, repo: string) {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: { plugin: "list", title, origin: { repo, workflow: "layout" }, requested_by: "spec", payload },
  });
  expect(response.status(), await response.text()).toBe(201);
}

/** Discards whatever is pending, so a test starts from an empty inbox. */
async function clearInbox(request: APIRequestContext) {
  const pending = (await (await request.get(`${core}/api/v1/reviews?status=pending&limit=500`)).json()).reviews as { id: string }[];
  for (const r of pending) await request.post(`${core}/api/v1/reviews/${r.id}/discard`, { data: { reason: "spec cleanup" } });
}

test("the inbox is grouped by project or one list, newest first, and remembers which", async ({ page }) => {
  await clearInbox(page.request);
  // oldest first: zeta gets the oldest and the newest, acme the one between
  await createReview(page.request, "Layout one", "zeta/app");
  await createReview(page.request, "Layout two", "acme/api");
  await createReview(page.request, "Layout three", "zeta/app");

  await page.goto("/#/");
  const rows = page.locator("[data-review-row]");
  // grouped: the project with the newest review first, then newest within it
  await expect(page.locator(".inbox-repo summary strong")).toHaveText(["zeta/app", "acme/api"]);
  await expect(rows).toHaveText([/Layout three/, /Layout one/, /Layout two/]);

  await page.getByRole("radio", { name: "One list" }).click();
  await expect(page.locator(".inbox-repo")).toHaveCount(0);
  await expect(rows).toHaveText([/Layout three/, /Layout two/, /Layout one/]);
  // with no project headings, each row says its project
  await expect(rows.nth(1).locator(".review-row-project")).toHaveText("acme/api");

  // J walks the list as it is drawn
  await page.keyboard.press("j");
  await expect(rows.nth(1)).toHaveClass(/is-focused/);

  // the choice outlasts a reload
  await page.reload();
  await expect(page.getByRole("radio", { name: "One list" })).toHaveAttribute("aria-checked", "true");
  await expect(rows).toHaveText([/Layout three/, /Layout two/, /Layout one/]);

  await page.getByRole("radio", { name: "Grouped by project" }).click();
  await expect(page.locator(".inbox-repo")).toHaveCount(2);
});
