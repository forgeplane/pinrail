// A session's header keeps its height whatever it shows, so that the
// terminal under it is not resized.

import { expect, test, type Page } from "../fixtures";
import { core } from "../helpers";

let n = 0;
const fresh = () => `s-head-${Date.now()}-${++n}`;

const headHeight = (page: Page) => page.locator(".thread-head").evaluate((el) => el.getBoundingClientRect().height);

test("the header keeps its height when the session takes and releases items, and when it finishes", async ({
  page,
  request,
}) => {
  const session = fresh();
  const post = async (path: string, data: unknown) => {
    const response = await request.post(`${core}/api/v1/${path}`, { data });
    expect(response.status(), await response.text()).toBeLessThan(300);
    return response;
  };
  await post(`sessions/claude-code/${session}/status`, { kind: "start", text: "Keep the header still" });
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(page.getByRole("heading", { name: "Keep the header still" })).toBeVisible();
  const before = await headHeight(page);

  // it takes items: the working-on line shows
  const repo = `head-${Date.now()}/repo`;
  for (const title of ["First item", "Second item"]) {
    const item = (await (await post("items", { repo, title, by: "claude-code", session })).json()) as { key: string };
    await request.patch(`${core}/api/v1/items/${item.key}`, { data: { by: "claude-code", session, claim: "claim" } });
  }
  await expect(page.getByTestId("thread-held-item")).toBeVisible();
  expect(await headHeight(page)).toBe(before);

  // it finishes: the presence chip changes
  await post(`sessions/claude-code/${session}/status`, { kind: "done", text: "Done" });
  await expect(page.getByText("Done").first()).toBeVisible();
  expect(await headHeight(page)).toBe(before);
});
