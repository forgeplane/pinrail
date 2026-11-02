// The button in a session's header copies the session's id.

import { expect, test } from "@playwright/test";
import { core } from "./helpers";

test.use({ permissions: ["clipboard-read", "clipboard-write"] });

test("the header copies the session id", async ({ page }) => {
  const session = `s-copy-${Date.now()}`;
  const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
    data: { text: "Copy this session", kind: "start" },
  });
  expect(response.status(), await response.text()).toBe(200);
  await page.goto(`/#/sessions/claude-code/${session}`);

  await page.getByTestId("thread").getByRole("button", { name: "Copy the session id" }).click();

  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(session);
});
