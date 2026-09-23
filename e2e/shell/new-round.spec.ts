import { expect, test } from "@playwright/test";

const core = "http://127.0.0.1:4799";

const payload = (title: string) => ({ groups: [{ title: "lib/acme/tickets.ex", items: [{ id: 1, title }] }] });

test("a new round of the open review shows in the switcher and says it is waiting", async ({ page }) => {
  const submit = async (title: string, revises?: string) => {
    const response = await page.request.post(`${core}/api/v1/reviews`, {
      data: { plugin: "list", title, origin: { repo: "acme/api" }, revises, payload: payload(title) },
    });
    expect(response.status(), await response.text()).toBe(201);
    return ((await response.json()) as { id: string }).id;
  };
  const decide = async (id: string) => {
    const response = await page.request.post(`${core}/api/v1/reviews/${id}/decision`, {
      data: { data: { decisions: [{ id: 1, action: "accept" }], undecided: [] } },
    });
    expect(response.status(), await response.text()).toBe(200);
  };

  const first = await submit("Rounds: one");
  await decide(first);
  const second = await submit("Rounds: two", first);
  await decide(second);

  // round 1 is open when round 3 arrives: it revises round 2, not the open one
  await page.goto(`/#/reviews/${first}`);
  const pills = page.locator(".rounds .round-pill");
  await expect(pills).toHaveCount(2);
  await expect(page.locator("[data-new-round]")).toHaveCount(0);

  const third = await submit("Rounds: three", second);
  await expect(pills).toHaveCount(3);
  await expect(pills.nth(2)).toHaveClass(/is-waiting/);
  // each round in the colour of its outcome
  await expect(pills.nth(0)).toHaveAttribute("data-outcome", "decided");
  await expect(pills.nth(2)).toHaveAttribute("data-outcome", "pending");
  const banner = page.locator("[data-new-round]");
  await expect(banner).toContainText("Round 3 is waiting for you.");

  await banner.getByRole("link", { name: "Open it" }).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${third}$`));
  await expect(page.locator("[data-new-round]")).toHaveCount(0);
  await expect(pills.nth(2)).toHaveClass(/is-current/);

  // once opened it is no longer news: going back to round 1 says nothing
  await pills.nth(0).click();
  await expect(page).toHaveURL(new RegExp(`/reviews/${first}$`));
  await expect(pills.nth(0)).toHaveClass(/is-current/);
  await expect(page.locator("[data-new-round]")).toHaveCount(0);
  await expect(pills.nth(2)).not.toHaveClass(/is-waiting/);
});
