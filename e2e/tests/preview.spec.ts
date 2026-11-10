import { expect, test, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { scratch } from "../helpers/scratch";
import { loadState } from "../helpers/state";

// A review in a browser, as an agent with a browser tool sees it: the
// plugin's view in a frame, fed the review, and a hand-over that checks the
// decision and stores nothing; deciding is the app's.
test("a review's preview shows its view and checks its hand-over, deciding nothing", async ({ page }) => {
  const { url } = loadState();
  const sample = await (
    await fetch(`${url}/api/v1/plugins/list/sample`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: "{}",
    })
  ).json();

  await page.goto(`${url}/preview/reviews/${sample.id}`);
  await expect(page.locator("#title")).toHaveText(sample.title);
  await expect(page.locator("#app")).toHaveAttribute("href", `pinrail://reviews/${sample.id}`);
  const view = page.frameLocator("#view");
  await expect(view.getByText("Ecto.StaleEntryError").first()).toBeVisible();

  // the list asks again while items are undecided, through the button's label
  await page.locator("#handover").click();
  await expect(page.locator("#handover")).toHaveText("Check: Hand over with 4 undecided");
  await page.locator("#handover").click();
  await expect(page.locator("#result p")).toHaveText(/^The decision passes\./);
  const shown = JSON.parse(await page.locator("#result pre").innerText());
  expect(shown.undecided.length).toBeGreaterThan(0);

  const after = await (await fetch(`${url}/api/v1/reviews/${sample.id}`)).json();
  expect(after.status).toBe("pending");
});

test("a preview of no review says so", async ({ page }) => {
  const { url } = loadState();
  await page.goto(`${url}/preview/reviews/r_nope`);
  await expect(page.locator("main")).toHaveText("No review r_nope.");
});

/** Installs a plugin linked from a scratch folder, with `script` as its
 *  view's, and opens a preview of a review of it. */
async function previewOf(page: Page, name: string, manifest: object, script: string) {
  const { url } = loadState();
  const dir = scratch(`pinrail-${name}-`);
  fs.mkdirSync(path.join(dir, "view"), { recursive: true });
  fs.mkdirSync(path.join(dir, "schemas"), { recursive: true });
  fs.writeFileSync(path.join(dir, "schemas", "payload.schema.json"), "{}");
  fs.writeFileSync(path.join(dir, "schemas", "decision.schema.json"), "{}");
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ name, version: "1.0.0", ...manifest }));
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html><meta charset="utf-8"><script src="/sdk/v1/pinrail-plugin.js"></script>
<button id="go">Go</button><pre id="out"></pre>
<script>
  const plugin = Pinrail.connect({});
  ${script}
</script>`,
  );
  const installed = await fetch(`${url}/api/v1/plugins/install`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ source: dir, link: true }),
  });
  expect(installed.status, await installed.text()).toBe(200);
  const created = await (
    await fetch(`${url}/api/v1/reviews`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ plugin: name, title: `A review of ${name}`, requested_by: "spec", payload: {} }),
    })
  ).json();
  await page.goto(`${url}/preview/reviews/${created.id}`);
  return page.frameLocator("#view");
}

// The preview copies for a view whose manifest declares the clipboard, as
// the app does, with the browser's own clipboard.
test("a preview copies the text of a view that declares the clipboard", async ({ page, context }) => {
  const { url } = loadState();
  await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: url });
  const view = await previewOf(
    page,
    "preview-copier",
    { capabilities: ["clipboard"] },
    `document.getElementById("go").onclick = () =>
    plugin.copy({ text: "from the preview" }).then(
      () => (document.getElementById("out").textContent = "copied"),
      (e) => (document.getElementById("out").textContent = "refused: " + e.message),
    );`,
  );
  await view.getByRole("button", { name: "Go" }).click();
  await expect(view.locator("#out")).toHaveText("copied");
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("from the preview");
});

// A link can carry a review's content out, so the preview asks before it
// opens one, as the app does.
test("a preview asks before it opens a view's link", async ({ page, context }) => {
  const link = "https://example.com/?d=the-review";
  await context.route("https://example.com/**", (route) => route.fulfill({ body: "" }));
  const view = await previewOf(
    page,
    "preview-opener",
    {},
    `document.getElementById("go").onclick = () => plugin.open(${JSON.stringify(link)});`,
  );
  const opened: string[] = [];
  context.on("page", (p) => opened.push(p.url()));

  const asked: string[] = [];
  page.once("dialog", (dialog) => {
    asked.push(dialog.message());
    void dialog.dismiss();
  });
  await view.getByRole("button", { name: "Go" }).click();
  await expect.poll(() => asked.length).toBe(1);
  expect(asked[0]).toContain("example.com");
  await page.waitForTimeout(300);
  expect(opened).toEqual([]);

  page.once("dialog", (dialog) => void dialog.accept());
  const popup = context.waitForEvent("page");
  await view.getByRole("button", { name: "Go" }).click();
  await (await popup).waitForURL(link);
});
