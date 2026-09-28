import { expect, test } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { clearInbox, core, createReview, linkPlugin } from "./helpers";
import { scratch } from "../helpers/scratch";

/** A view that sends its frame to another page, which asks for the review
 *  and tries to hand over a decision. */
function wanderer(): string {
  const dir = scratch("pinrail-wanderer-");
  fs.mkdirSync(path.join(dir, "view"));
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({ name: "wanderer", version: "1.0.0", title: "Wanderer", entry: "view/index.html", payload_schema: {}, decision_schema: {} }),
  );
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html><meta charset="utf-8"><script src="/sdk/v1/pinrail-plugin.js"></script>
<script>Pinrail.connect({ onInit() { location.href = "elsewhere.html"; } });</script>`,
  );
  fs.writeFileSync(
    path.join(dir, "view", "elsewhere.html"),
    `<!doctype html><meta charset="utf-8"><p id="got">nothing</p>
<script>
  window.addEventListener("message", (e) => {
    if (e.data && e.data.type === "init") document.getElementById("got").textContent = "the review: " + e.data.gate.title;
  });
  parent.postMessage({ pinrail: 1, type: "ready" }, "*");
  parent.postMessage({ pinrail: 1, type: "submit", data: {} }, "*");
</script>`,
  );
  return dir;
}

test("a view that leaves its page is no longer answered", async ({ page }) => {
  await linkPlugin(page.request, wanderer(), "wanderer");
  await clearInbox(page.request);
  const { id } = await createReview(page.request, { plugin: "wanderer", title: "Stay put", payload: {} });

  await page.goto(`/#/reviews/${id}`);
  // the app notices, and hides the page the view went to
  await expect(page.locator("[data-view-left]")).toBeVisible();
  await expect(page.locator("#plugin-frame")).toBeHidden();
  const other = page.frameLocator("#plugin-frame").locator("#got");
  await expect(other).toHaveText("nothing");
  await page.waitForTimeout(500);
  await expect(other).toHaveText("nothing");
  const review = await (await page.request.get(`${core}/api/v1/reviews/${id}`)).json();
  expect(review.status, "the other page's hand-over was not taken").toBe("pending");

  // reloading brings the view's own page back, which here leaves again
  await page.locator("[data-view-left] button").click();
  await expect(page.locator("[data-view-left]")).toBeVisible();
  await expect(other).toHaveText("nothing");
});
