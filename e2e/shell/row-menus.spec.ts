import { expect, test, type Locator } from "@playwright/test";
import { createReview, decide } from "./helpers";

const approved = { decisions: [{ id: 1, action: "accept" }], undecided: [] };
const opacity = (button: Locator) => button.evaluate((b) => getComputedStyle(b).opacity);
const background = (row: Locator) => row.evaluate((r) => getComputedStyle(r).backgroundColor);

test("an open menu keeps its history row marked while the pointer moves over others", async ({ page, request }) => {
  const run = Date.now();
  for (const title of ["one", "two"]) {
    const { id } = await createReview(request, { title: `Row menu ${title} ${run}` });
    await decide(request, id, approved);
  }
  await page.goto(`/#/?tab=history&q=${run}`);
  const one = page.getByTestId("history-row").filter({ hasText: `Row menu one ${run}` });
  const two = page.getByTestId("history-row").filter({ hasText: `Row menu two ${run}` });
  const button = one.getByRole("button", { name: /actions/ });
  await one.hover();
  await button.click();
  await expect(page.getByRole("menu")).toBeVisible();
  await two.hover();
  await expect(one).toHaveClass(/is-focused/);
  await expect(two).not.toHaveClass(/is-focused/);
  await expect(button).toHaveAttribute("aria-expanded", "true");
  expect(await opacity(button)).toBe("1");
  expect(await opacity(two.getByRole("button", { name: /actions/ }))).toBe("0");
});

test("an open menu keeps its repository row marked while the pointer moves over others", async ({ page, request }) => {
  const run = Date.now();
  for (const name of ["one", "two"])
    await createReview(request, { title: "x", origin: { repo: `row-menu/${name}-${run}` } });
  await page.goto("/#/projects");
  const row = (name: string) => page.getByTestId("project-repo").filter({ hasText: `row-menu/${name}-${run}` });
  const button = row("one").getByRole("button", { name: /actions/ });
  await row("one").hover();
  await button.click();
  await expect(page.getByRole("menu")).toBeVisible();
  await row("two").hover();
  const highlight = await background(row("one"));
  expect(highlight).not.toBe("rgba(0, 0, 0, 0)");
  expect(await background(row("two"))).toBe("rgba(0, 0, 0, 0)");
  expect(await opacity(button)).toBe("1");
  expect(await opacity(row("two").getByRole("button", { name: /actions/ }))).toBe("0");
});
