// The table style and width in Settings › Appearance, as a table in a work
// item's description shows them.

import { expect, test } from "../fixtures";
import { core, settingSaved } from "../helpers";

test.afterEach(async ({ request }) => {
  await request.patch(`${core}/api/v1/settings`, { data: { appearance: { tables: "bordered", table_width: "fit" } } });
  await settingSaved(request, (s) => s.appearance.tables, "bordered");
});

test("Settings changes how tables look and how wide they are", async ({ page, request }) => {
  const repo = `tables-${Date.now()}/repo`;
  const added = await request.post(`${core}/api/v1/items`, {
    data: {
      repo,
      title: "Review the merge requests",
      by: "claude-code",
      body: "| MR | Review |\n| --- | --- |\n| !7281 | Re-review |\n| !7262 | Re-review |\n| !7327 | Full review |",
    },
  });
  expect(added.status(), await added.text()).toBe(201);
  const { key } = (await added.json()) as { key: string };

  await page.goto(`/#/items?repo=${encodeURIComponent(repo)}&open=${key}`);
  const description = page.getByTestId("item-panel").locator(".item-md");
  const table = description.locator("table");
  await expect(table).toBeVisible();
  const style = (selector: string, property: string) =>
    table
      .locator(selector)
      .first()
      .evaluate((el, p) => getComputedStyle(el).getPropertyValue(p), property);

  // bordered, the default: a line around every cell
  expect(await style("th", "border-left-width")).toBe("1px");

  await page.keyboard.press("ControlOrMeta+,");
  await page.getByRole("button", { name: "Appearance" }).click();
  const tables = page.getByRole("radiogroup", { name: "Tables" });

  // lines: no vertical lines, a line under each row
  await tables.getByRole("radio", { name: "Lines" }).click();
  await expect.poll(() => style("th", "border-left-width")).toBe("0px");
  expect(await style("td", "border-bottom-width")).toBe("1px");
  await settingSaved(request, (s) => s.appearance.tables, "lines");

  // striped: no lines between rows, every other row tinted
  await tables.getByRole("radio", { name: "Striped" }).click();
  await expect.poll(() => style("td", "border-bottom-width")).toBe("0px");
  const plain = await style("tbody tr:nth-child(1) td", "background-color");
  expect(await style("tbody tr:nth-child(2) td", "background-color")).not.toBe(plain);

  // full width stretches the table across the description
  const width = await description.evaluate((el) => el.clientWidth);
  expect((await table.boundingBox())!.width).toBeLessThan(width / 2);
  await page.getByRole("radiogroup", { name: "Table width" }).getByRole("radio", { name: "Full width" }).click();
  await expect.poll(async () => Math.round((await table.boundingBox())!.width)).toBe(width);
  await settingSaved(request, (s) => s.appearance.table_width, "full");
});
