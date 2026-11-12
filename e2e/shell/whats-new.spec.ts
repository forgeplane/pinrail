import { expect, test, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { core, settingsDialog } from "./helpers";

// a route handler may still be reading a response when the test ends, and
// closing the page disposes it: such a handler's error is not the test's
test.afterEach(({ page }) => page.unrouteAll({ behavior: "ignoreErrors" }));

/** Makes the app run as the given release by changing the version the core reports. */
async function runAs(page: Page, version: string) {
  await page.route(`${core}/api/v1/info`, async (route) => {
    const info = await (await route.fetch()).json();
    await route.fulfill({ json: { ...info, version } });
  });
}

async function setSeen(page: Page, seen: string | null) {
  const response = await page.request.patch(`${core}/api/v1/settings`, { data: { whats_new: { seen } } });
  expect(response.status(), await response.text()).toBe(200);
}

const seenNow = async (page: Page) =>
  ((await (await page.request.get(`${core}/api/v1/settings`)).json()) as { whats_new: { seen: string | null } })
    .whats_new.seen;

// every other spec expects What's new to be marked as seen for any release
test.afterEach(({ page }) => setSeen(page, "999.0.0"));

test("a new release shows its cards once, one at a time, and a card opens what it is about", async ({ page }) => {
  await setSeen(page, "0.2.0");
  await runAs(page, "0.3.0");
  await page.goto("/#/");

  const dialog = page.getByTestId("whats-new");
  const card = dialog.getByTestId("whats-new-card");
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("New in 0.3.0");
  await expect(card).toHaveText("Reviews open beside the inbox");
  // the image matches the theme
  // a production build adds a hash to the file name
  await expect(dialog.locator("img")).toHaveAttribute("src", /inbox-panel-(light|dark)[-.\w]*\.webp/);

  await dialog.getByTestId("whats-new-next").click();
  await expect(card).toHaveText("See where a decision came from");
  await page.keyboard.press("ArrowRight");
  await expect(card).toHaveText("Official plugins update with the app");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowRight");

  // a card's button closes the dialog and opens the screen the card describes
  await dialog.getByRole("button", { name: "Open Plugins" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(settingsDialog(page).getByRole("heading", { name: "Plugins" })).toBeVisible();
  await expect.poll(() => seenNow(page)).toBe("0.3.0");

  // once seen, the dialog does not appear again
  await page.reload();
  await expect(page.getByTestId("inbox-total")).toBeVisible();
  await expect(dialog).toHaveCount(0);
});

test("Esc closes the dialog and records the release as seen", async ({ page }) => {
  await setSeen(page, "0.2.0");
  await runAs(page, "0.3.0");
  await page.goto("/#/");
  const dialog = page.getByTestId("whats-new");
  await expect(dialog).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect.poll(() => seenNow(page)).toBe("0.3.0");
});

test("a release with no notes shows nothing, and counts as seen", async ({ page }) => {
  await setSeen(page, "0.3.0");
  await runAs(page, "0.3.1");
  await page.goto("/#/");
  await expect(page.getByTestId("inbox-total")).toBeVisible();
  await expect.poll(() => seenNow(page)).toBe("0.3.1");
  await expect(page.getByTestId("whats-new")).toHaveCount(0);
});

/** The newest release with notes, from the notes the app carries. */
const newest = fs
  .readdirSync(path.join(__dirname, "..", "..", "desktop", "app", "ui", "src", "whats-new"))
  .flatMap((f) => (/^\d+\.\d+\.\d+\.json$/.test(f) ? [f.replace(/\.json$/, "")] : []))
  .sort((a, b) => {
    const [x, y] = [a, b].map((v) => v.split(".").map(Number));
    return x[0] - y[0] || x[1] - y[1] || x[2] - y[2];
  })
  .pop();
/** The first card of the newest notes, which the dialog opens on. */
const firstCard = (
  JSON.parse(
    fs.readFileSync(
      path.join(__dirname, "..", "..", "desktop", "app", "ui", "src", "whats-new", `${newest}.json`),
      "utf8",
    ),
  ) as { cards: { title: string }[] }
).cards[0].title;

test("the newest notes open again from ⌘K and from Settings › About, without changing what was seen", async ({
  page,
}) => {
  // the suite serves a production build, which shows notes only up to the
  // release it runs as
  await runAs(page, newest!);
  await page.goto("/#/");
  await expect(page.getByTestId("inbox-total")).toBeVisible();
  const dialog = page.getByTestId("whats-new");
  await expect(dialog).toHaveCount(0);

  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Search" });
  await palette.getByRole("textbox").fill("What's new");
  await palette.getByRole("option", { name: /What's new in Pinrail/ }).click();
  await expect(dialog).toContainText(`New in ${newest}`);
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);

  await page.keyboard.press("ControlOrMeta+,");
  await settingsDialog(page)
    .getByRole("navigation", { name: "Settings sections" })
    .getByRole("button", { name: "About" })
    .click();
  await settingsDialog(page).getByRole("button", { name: "What's new" }).click();
  await expect(settingsDialog(page)).toHaveCount(0);
  await expect(dialog.getByTestId("whats-new-card")).toHaveText(firstCard);
  await dialog.getByRole("button", { name: "Close" }).click();

  expect(await seenNow(page)).toBe("999.0.0");
});
