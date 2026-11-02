// The stages in a session's details: the latest shown, and the earlier ones
// folded behind a button.

import { expect, test, type Page } from "@playwright/test";
import { core } from "./helpers";

let n = 0;
/** A session of its own for each test. */
const fresh = () => `s-stages-${Date.now()}-${++n}`;

/** A session that started this many stages, named "<tag> stage 1" and on. */
async function startStages(page: Page, session: string, tag: string, count: number) {
  for (let i = 1; i <= count; i++) {
    const response = await page.request.post(`${core}/api/v1/sessions/claude-code/${session}/status`, {
      data: { text: `${tag} stage ${i}`, kind: "start", origin: { repo: "acme/api" } },
    });
    expect(response.status(), await response.text()).toBe(200);
  }
}

const stagesIn = (page: Page) => page.getByRole("region", { name: "Stages" });

test("the details show the latest 8 stages and fold the earlier ones", async ({ page }) => {
  const session = fresh();
  await startStages(page, session, "fold", 11);
  await page.goto(`/#/sessions/claude-code/${session}`);
  const stages = stagesIn(page).getByTestId("stage");
  await expect(stages).toHaveCount(8);
  await expect(stages.first()).toContainText("fold stage 4");
  await expect(stages.last()).toContainText("fold stage 11");

  await stagesIn(page).getByRole("button", { name: "3 earlier stages" }).click();
  await expect(stages).toHaveCount(11);
  await expect(stages.first()).toContainText("fold stage 1");
  await stagesIn(page).getByRole("button", { name: "Show only the latest" }).click();
  await expect(stages).toHaveCount(8);
});

test("a session with 8 stages folds none", async ({ page }) => {
  const session = fresh();
  await startStages(page, session, "few", 8);
  await page.goto(`/#/sessions/claude-code/${session}`);
  await expect(stagesIn(page).getByTestId("stage")).toHaveCount(8);
  await expect(stagesIn(page).getByRole("button")).toHaveCount(0);
});

test("a thread opened at an earlier stage shows every stage", async ({ page }) => {
  const tag = `early${Date.now()}`;
  const session = fresh();
  await startStages(page, session, tag, 1);
  await startStages(page, session, "later", 9);
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await palette.getByRole("textbox").fill(tag);
  await palette.getByRole("option", { name: new RegExp(`${tag} stage 1`) }).click();
  await expect(stagesIn(page).getByTestId("stage")).toHaveCount(10);
});
