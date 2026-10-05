import { expect, test } from "@playwright/test";
import { core } from "./helpers";

test("the setup installs the plugins it recommends, and the first review asks for list until it is there", async ({
  page,
}) => {
  // a person who has installed nothing yet
  for (const name of ["list", "feedback"]) {
    expect((await page.request.delete(`${core}/api/v1/plugins/${name}`)).status()).toBe(200);
  }
  await page.goto("/");
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await palette.getByRole("textbox").fill("Set up Pinrail");
  await palette.getByRole("option", { name: /Set up Pinrail/ }).click();
  const setup = page.getByRole("dialog", { name: "Set up Pinrail" });

  // the first review needs list, and says so
  await setup.getByRole("button", { name: /A first review/ }).click();
  await expect(setup.locator("[data-welcome-needs-list]")).toContainText("which is not installed");

  await setup.getByRole("button", { name: /Plugins/ }).click();
  const plugins = setup.locator("[data-welcome-plugins]");
  for (const name of ["list", "feedback"]) {
    await expect(plugins.locator(`[data-welcome-plugin="${name}"]`)).toContainText("Recommended");
    await expect(plugins.locator(`[data-welcome-plugin="${name}"] input`)).toBeChecked();
  }
  // the others are offered, not chosen
  await expect(plugins.locator('[data-welcome-plugin="notes"] input')).not.toBeChecked();
  await setup.locator("[data-welcome-install]").click();
  for (const name of ["list", "feedback"]) {
    await expect(plugins.locator(`[data-welcome-plugin="${name}"]`)).toContainText("Installed");
  }
  await expect(setup.locator("[data-welcome-install]")).toBeDisabled();

  await setup.getByRole("button", { name: /A first review/ }).click();
  await expect(setup.locator("[data-welcome-needs-list]")).toHaveCount(0);
  await expect(setup.getByText("Waiting for the review to arrive")).toBeVisible();
  await page.keyboard.press("Escape");
});
