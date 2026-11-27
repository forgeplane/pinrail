// A review opened from a work item's side panel, which shows it in the
// item's place, on the Work items page and beside a session's thread.

import { expect, test, type APIRequestContext, type Page } from "../fixtures";
import { core, openReviewsIn, settled } from "../helpers";

let counter = 0;
const fresh = (name = "repo") => `rb-${Date.now()}-${++counter}/${name}`;

async function add(request: APIRequestContext, repo: string, title: string, extra: Record<string, unknown> = {}) {
  const response = await request.post(`${core}/api/v1/items`, {
    data: { repo, title, by: "claude-code", session: "s-beside", ...extra },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { key: string };
}

async function review(request: APIRequestContext, repo: string, title: string, items: string[]) {
  const response = await request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin: "list",
      title,
      origin: { repo },
      requested_by: "claude-code",
      items,
      payload: { groups: [{ title: "g", items: [{ id: 1, title: "x" }] }] },
    },
  });
  expect(response.status(), await response.text()).toBe(201);
  return (await response.json()) as { id: string };
}

const discard = (request: APIRequestContext, id: string) =>
  request.post(`${core}/api/v1/reviews/${id}/discard`, { data: { reason: "spec cleanup" } });

const itemPanel = (page: Page) => page.getByTestId("item-panel");
const strip = (page: Page) => page.getByTestId("review-strip");

test.beforeEach(({ request }) => openReviewsIn(request, "panel"));

test("on the Work items page, a review opens in the item's place and closes back to it", async ({ page, request }) => {
  const repo = fresh("page");
  const item = await add(request, repo, "Theme loader");
  const sent = await review(request, repo, "Loader review", [item.key]);
  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}&open=${item.key}`);
  await itemPanel(page)
    .getByRole("region", { name: "Reviews" })
    .getByRole("link", { name: /Loader review/ })
    .click();

  // the list stays, and the panel shows the review
  await expect(strip(page)).toContainText(sent.id);
  await expect(page).toHaveURL(new RegExp(`#/items\\?.*review=${sent.id}`));
  await expect(page.getByTestId("item-row").filter({ hasText: "Theme loader" })).toBeVisible();
  await expect(itemPanel(page)).toHaveCount(0);

  // the item's keys do not reach the item behind the review
  await page.getByTestId("item-row").filter({ hasText: "Theme loader" }).hover();
  await page.keyboard.press("d");
  // any change the key made would be a request to the API
  await settled(page, (url) => url.pathname.startsWith("/api/"));
  const shown = (await (await request.get(`${core}/api/v1/items/${item.key}`)).json()) as { status: string };
  expect(shown.status).toBe("open");

  // closing the review shows the item again
  await page.keyboard.press("Escape");
  await expect(itemPanel(page).locator(".item-panel-key")).toHaveText(item.key);
  await expect(strip(page)).toHaveCount(0);
  await discard(request, sent.id);
});

test("beside a session's thread, a review opens in the item's place", async ({ page, request }) => {
  const session = `s-beside-${Date.now()}`;
  const started = await request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { kind: "start", text: "Load the themes" },
  });
  expect(started.status(), await started.text()).toBe(200);
  const repo = fresh("thread");
  const item = await add(request, repo, "Theme picker", { session });
  const sent = await review(request, repo, "Picker review", [item.key]);
  await page.goto(`/#/sessions/claude-code/${session}?item=${item.key}`);
  await itemPanel(page)
    .getByRole("region", { name: "Reviews" })
    .getByRole("link", { name: /Picker review/ })
    .click();
  await expect(strip(page)).toContainText(sent.id);
  await expect(page).toHaveURL(new RegExp(`#/sessions/claude-code/[^?]+\\?item=${item.key}&review=${sent.id}`));
  await expect(page.getByTestId("thread")).toBeVisible();
  await expect(itemPanel(page)).toHaveCount(0);

  // closing the review shows the item again
  await page.keyboard.press("Escape");
  await expect(itemPanel(page).locator(".item-panel-key")).toHaveText(item.key);
  await discard(request, sent.id);
});
