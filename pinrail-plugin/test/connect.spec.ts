import { expect, test } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { reviewFrom, mountPlugin } from "@forgeplane/pinrail-plugin/testing";
import { scratch } from "./scratch.cjs";

// A document connects to the app once.

/** A plugin whose view connects twice, and shows what the second call did. */
function twice(): string {
  const dir = scratch("pinrail-connect-");
  fs.mkdirSync(path.join(dir, "view"), { recursive: true });
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ name: "twice", version: "1.0.0" }));
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html>
<meta charset="utf-8">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<p id="title"></p><p id="second"></p>
<script>
  const plugin = Pinrail.connect({ onInit({ review }) { document.getElementById("title").textContent = review.title; } });
  try {
    Pinrail.connect({});
    document.getElementById("second").textContent = "connected again";
  } catch (error) {
    document.getElementById("second").textContent = error.message;
  }
</script>`,
  );
  return dir;
}

test("a second connect throws with what to do, and posts nothing", async ({ page }) => {
  const plugin = await mountPlugin(page, twice(), { review: reviewFrom({ title: "Once", payload: {} }) });
  await expect(plugin.frame.locator("#title")).toHaveText("Once");
  await expect(plugin.frame.locator("#second")).toHaveText(
    "Pinrail.connect was called twice in this document: connect once, where the page starts, and keep the plugin it returns",
  );
  // the app hears one ready: the first connection stands
  const ready = (await plugin.messages()).filter((m) => m.type === "ready");
  expect(ready).toHaveLength(1);
});
