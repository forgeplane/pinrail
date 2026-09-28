import { expect, test } from "@playwright/test";
import fs from "node:fs";
import http from "node:http";
import type { AddressInfo } from "node:net";
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
    JSON.stringify({
      name: "wanderer",
      version: "1.0.0",
      title: "Wanderer",
      entry: "view/index.html",
      payload_schema: {},
      decision_schema: {},
    }),
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
    if (e.data && e.data.type === "init") document.getElementById("got").textContent = "the review: " + e.data.review.title;
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

/** A view whose buttons send its frame elsewhere: another program's port on
 *  this computer, or a site on the internet. */
function traveller(localUrl: string): string {
  const dir = scratch("pinrail-traveller-");
  fs.mkdirSync(path.join(dir, "view"));
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({
      name: "traveller",
      version: "1.0.0",
      title: "Traveller",
      entry: "view/index.html",
      payload_schema: {},
      decision_schema: {},
    }),
  );
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html><meta charset="utf-8"><script src="/sdk/v1/pinrail-plugin.js"></script>
<button id="local">local</button><button id="external">external</button>
<script>
  Pinrail.connect({});
  document.getElementById("local").onclick = () => { location.href = ${JSON.stringify(localUrl)}; };
  document.getElementById("external").onclick = () => { location.href = "https://example.com/"; };
</script>`,
  );
  return dir;
}

test("the frame can show only Pinrail's own server", async ({ page }) => {
  // another program on this computer, which counts the requests it gets
  const hits: string[] = [];
  const server = http.createServer((req, res) => {
    hits.push(req.url ?? "");
    res.end("<p>another program</p>");
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = (server.address() as AddressInfo).port;
  try {
    await linkPlugin(page.request, traveller(`http://127.0.0.1:${port}/`), "traveller");
    await clearInbox(page.request);
    const requested: string[] = [];
    page.on("request", (r) => requested.push(r.url()));

    for (const button of ["#local", "#external"]) {
      const { id } = await createReview(page.request, { plugin: "traveller", title: `Travel ${button}`, payload: {} });
      await page.goto(`/#/reviews/${id}`);
      await page.frameLocator("#plugin-frame").locator(button).click();
      await page.waitForTimeout(1000);
    }
    expect(hits, "the other program was asked for a page").toEqual([]);
    expect(
      requested.filter((u) => u.startsWith("https://example.com")),
      "the external site was requested",
    ).toEqual([]);
  } finally {
    server.close();
  }
});

/** A view that leaves its page only when asked, for another page of its own. */
function leaver(): string {
  const dir = scratch("pinrail-leaver-");
  fs.mkdirSync(path.join(dir, "view"));
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({
      name: "leaver",
      version: "1.0.0",
      title: "Leaver",
      entry: "view/index.html",
      payload_schema: {},
      decision_schema: {},
    }),
  );
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html><meta charset="utf-8"><script src="/sdk/v1/pinrail-plugin.js"></script>
<button id="leave">leave</button><button id="blocked">blocked</button>
<script>
  Pinrail.connect({});
  document.getElementById("leave").onclick = () => { location.href = "other.html"; };
  document.getElementById("blocked").onclick = () => { location.href = "https://example.com/"; };
</script>`,
  );
  fs.writeFileSync(path.join(dir, "view", "other.html"), `<!doctype html><p>another page</p>`);
  return dir;
}

for (const [button, where] of [
  ["#leave", "another of its pages"],
  ["#blocked", "a page the app blocks"],
]) {
  test(`Reload the view brings the view's own page back after it went to ${where}`, async ({ page }) => {
    await linkPlugin(page.request, leaver(), "leaver");
    await clearInbox(page.request);
    const { id } = await createReview(page.request, { plugin: "leaver", title: "Come back", payload: {} });
    await page.goto(`/#/reviews/${id}`);
    const view = page.frameLocator("#plugin-frame");
    await view.locator(button).click();
    await expect(page.locator("[data-view-left]")).toBeVisible();

    await page.locator("[data-view-left] button").click();
    await expect(page.locator("[data-view-left]")).toHaveCount(0);
    await expect(page.locator(".plugin-loading")).toHaveCount(0);
    await expect(view.locator("#leave")).toBeVisible();
  });
}
