// The Work items page shows its items a page at a time: 100 top-level
// items, each with its sub-items.

import { expect, test, type APIRequestContext } from "@playwright/test";
import { core } from "./helpers";

let n = 0;
const fresh = () => `pages-${Date.now()}-${++n}/api`;

/** Adds `count` items to `repo`, the first with two sub-items. */
async function add(request: APIRequestContext, repo: string, count: number) {
  // a breakdown holds at most 100 items: the first batch keeps room for the two sub-items
  for (let i = 0; i < count; i += i === 0 ? 98 : 100) {
    const items = Array.from({ length: Math.min(i === 0 ? 98 : 100, count - i) }, (_, j) => ({
      ref: `r${i + j}`,
      title: `Paged item ${String(i + j).padStart(3, "0")}`,
    }));
    if (i === 0) {
      items.push({ ref: "sub-a", title: "Paged sub-item a", parent: "r0" } as (typeof items)[number]);
      items.push({ ref: "sub-b", title: "Paged sub-item b", parent: "r0" } as (typeof items)[number]);
    }
    const response = await request.post(`${core}/api/v1/items`, { data: { repo, items, by: "spec" } });
    expect(response.status(), await response.text()).toBe(201);
  }
}

test("the list shows 100 top-level items a page, each with its sub-items", async ({ page, request }) => {
  const repo = fresh();
  await add(request, repo, 120);
  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}`);
  const rows = page.getByTestId("item-row");
  // 100 top-level items, and the first one's two sub-items with it
  await expect(rows).toHaveCount(102);
  await expect(page.getByTestId("pager-range")).toHaveText("1–100 of 120");
  await expect(rows.filter({ hasText: "Paged sub-item a" })).toHaveCount(1);
  // the group's heading counts every item in it, not only the page's
  await expect(page.getByTestId("inbox-group-heading")).toContainText("122");

  await page.getByRole("button", { name: "Next page" }).click();
  await expect(rows).toHaveCount(20);
  await expect(page.getByTestId("pager-range")).toHaveText("101–120 of 120");
  await expect(page).toHaveURL(/page=2/);

  // a filter changed starts again at the first page
  // "11" matches 011 and 110 to 119
  await page.getByPlaceholder(/Search/).fill("Paged item 11");
  await expect(rows).toHaveCount(11);
  await expect(page).not.toHaveURL(/page=/);
  await expect(page.getByTestId("pager-range")).toHaveCount(0);
});
