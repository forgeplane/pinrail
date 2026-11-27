import { expect, test } from "../fixtures";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { core, linkPlugin, layout, settingsSection, toast } from "../helpers";
import { scratch } from "../../helpers/scratch";

// The clipboard capability: the app copies text, or an image the review
// carries, for a view whose manifest declares "clipboard", and for no other.

// WebKit knows clipboard-read only, and lets a page write without asking
test.use({
  permissions: async ({ browserName }, use) =>
    use(browserName === "webkit" ? ["clipboard-read"] : ["clipboard-read", "clipboard-write"]),
});

/** A one-pixel PNG. */
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=",
  "base64",
);

/** A plugin whose view copies its text and its image on a click, and says what happened. */
function copier(name: string, capabilities?: string[]): string {
  const dir = scratch(`pinrail-${name}-`);
  layout(dir);
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({
      name,
      version: "1.0.0",
      title: name,
      attachments: { accept: [".png"] },
      ...(capabilities ? { capabilities } : {}),
    }),
  );
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html><meta charset="utf-8"><script src="/sdk/v1/pinrail-plugin.js"></script>
<p id="can">waiting</p><button id="text">Copy the text</button><button id="image">Copy the image</button><pre id="out"></pre>
<script>
  const plugin = Pinrail.connect({
    onInit() {
      document.getElementById("can").textContent = plugin.canCopy ? "can copy" : "cannot copy";
    },
  });
  const report = (p) =>
    p.then(() => "copied", (e) => "refused: " + e.message).then((t) => (document.getElementById("out").textContent = t));
  document.getElementById("text").onclick = () => report(plugin.copy({ text: "Pinrail 0.3.0 is out." }));
  document.getElementById("image").onclick = () => report(plugin.copy({ attachment: "card.png" }));
</script>`,
  );
  return dir;
}

async function reviewWithImage(request: import("@playwright/test").APIRequestContext, plugin: string) {
  const sha256 = crypto.createHash("sha256").update(PNG).digest("hex");
  const put = await request.put(`${core}/api/v1/attachments/${sha256}`, {
    headers: { "content-type": "application/octet-stream" },
    data: PNG,
  });
  expect([200, 201]).toContain(put.status());
  const created = await request.post(`${core}/api/v1/reviews`, {
    data: {
      plugin,
      // a title of its own: the same submission while one is pending
      // answers that one, such as the other browser's
      title: `Copying from ${plugin} ${crypto.randomUUID()}`,
      requested_by: "spec",
      payload: { card: { $attachment: "card.png" } },
      attachments: { "card.png": { sha256, size: PNG.length, media_type: "image/png" } },
    },
  });
  expect(created.status(), await created.text()).toBe(201);
  return (await created.json()).id as string;
}

test("a view that declares the clipboard copies its text and an image of its review, with a toast for each", async ({
  page,
}) => {
  await linkPlugin(page.request, copier("copier", ["clipboard"]), "copier");
  const id = await reviewWithImage(page.request, "copier");
  await page.goto(`/#/reviews/${id}`);
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator("#can")).toHaveText("can copy");

  await frame.getByRole("button", { name: "Copy the text" }).click();
  await expect(frame.locator("#out")).toHaveText("copied");
  await expect(toast(page, "Copied the text")).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("Pinrail 0.3.0 is out.");

  await frame.getByRole("button", { name: "Copy the image" }).click();
  await expect(frame.locator("#out")).toHaveText("copied");
  await expect(toast(page, "Copied card.png")).toBeVisible();
  const types = await page.evaluate(async () => (await navigator.clipboard.read()).flatMap((item) => item.types));
  expect(types).toContain("image/png");
});

test("a view that does not declare the clipboard is not let copy", async ({ page }) => {
  await linkPlugin(page.request, copier("nocopy"), "nocopy");
  const id = await reviewWithImage(page.request, "nocopy");
  await page.goto(`/#/reviews/${id}`);
  const frame = page.frameLocator("#plugin-frame");
  await expect(frame.locator("#can")).toHaveText("cannot copy");
  await frame.getByRole("button", { name: "Copy the text" }).click();
  await expect(frame.locator("#out")).toHaveText(/^refused: the app does not let this plugin copy/);
});

test("Settings says which plugins can copy to the clipboard", async ({ page }) => {
  await linkPlugin(page.request, copier("copier", ["clipboard"]), "copier");
  await linkPlugin(page.request, copier("nocopy"), "nocopy");
  await page.goto("/#/");
  await page.keyboard.press("ControlOrMeta+,");
  await settingsSection(page, "Plugins").click();
  const copies = page.getByTestId("plugin-row-copier");
  await copies.getByRole("button", { name: "Details of copier" }).click();
  await expect(copies.getByTestId("plugin-clipboard")).toHaveText("Can copy text and images from its reviews");
  const other = page.getByTestId("plugin-row-nocopy");
  await other.getByRole("button", { name: "Details of nocopy" }).click();
  await expect(other.getByTestId("plugin-details")).toBeVisible();
  await expect(other.getByTestId("plugin-clipboard")).toHaveCount(0);
});
