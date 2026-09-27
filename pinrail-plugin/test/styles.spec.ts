import { expect, test } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { gateFrom, mountPlugin } from "@forgeplane/pinrail-plugin/testing";
import { scratch } from "./scratch.cjs";

// What the SDK's stylesheet gives every view, checked in a view's frame.

/** A plugin whose view is a button and the SDK's text fields. */
function fields(): string {
  const dir = scratch("pinrail-styles-");
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ name: "styles", version: 1, entry: "index.html" }));
  fs.writeFileSync(
    path.join(dir, "index.html"),
    `<!doctype html>
<meta charset="utf-8">
<link rel="stylesheet" href="/sdk/v1/pinrail-plugin.css">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<button class="btn" id="start">Start</button>
<input class="field" id="field" aria-label="field">
<textarea class="note" id="note" aria-label="note"></textarea>
<script>Pinrail.connect({ onInit() {} });</script>`,
  );
  return dir;
}

test("a text field reached from the keyboard shows the focus ring", async ({ page }) => {
  // the ring is how a keyboard user sees where they are
  const plugin = await mountPlugin(page, fields(), { gate: gateFrom({ title: "Styles", payload: {} }) });
  const f = plugin.frame;
  await f.locator("#start").focus();
  for (const id of ["field", "note"]) {
    await f.locator("body").press("Tab");
    await expect(f.locator(`#${id}`)).toBeFocused();
    const outline = await f.locator(`#${id}`).evaluate((el) => getComputedStyle(el).outlineStyle);
    expect(outline, `#${id} lost its focus ring`).not.toBe("none");
  }
});
