import { expect, test } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { mountPlugin } from "../harness/index.cjs";
import { scratch } from "./scratch.cjs";

// A view with a text field, and a key of its own it keeps from the app.
function view(): string {
  const dir = scratch("pinrail-app-keys-");
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ name: "keys", version: 1, title: "Keys", entry: "index.html" }));
  fs.writeFileSync(
    path.join(dir, "index.html"),
    `<!doctype html>
<meta charset="utf-8">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<p id="ready">waiting</p>
<textarea id="field"></textarea>
<script>
  Pinrail.connect({ onInit() { document.getElementById("ready").textContent = "ready"; } });
  // this view answers ] itself, so the app does not
  document.addEventListener("keydown", (e) => { if (e.key === "]") e.preventDefault(); });
</script>`,
  );
  return dir;
}

test("the app's keys go up from a view, unless typed in a field or kept by the view", async ({ page }) => {
  const plugin = await mountPlugin(page, view(), { gate: { payload: {} } });
  const f = plugin.frame;
  await expect(f.locator("#ready")).toHaveText("ready");
  const keys = async () => (await plugin.messages()).filter((m: any) => m.type === "key").map((m: any) => m.key);

  await f.locator("#ready").click();
  await page.keyboard.press("?");
  await page.keyboard.press("[");
  await page.keyboard.press("]");   // the view keeps this one
  await page.keyboard.press("j");   // not the app's
  await expect.poll(keys).toEqual(["?", "["]);

  await f.locator("#field").click();
  await page.keyboard.press("?");   // typed into the field
  await page.waitForTimeout(100);
  expect(await keys()).toEqual(["?", "["]);
});
