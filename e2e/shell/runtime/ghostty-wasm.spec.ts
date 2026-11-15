// The window loads libghostty-vt's WebAssembly under its own CSP, which
// allows compiling WebAssembly and nothing more than it did.

import fs from "node:fs";
import path from "node:path";
import { expect, test, type Page } from "../fixtures";

const conf = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../../../desktop/app/src-tauri/tauri.conf.json"), "utf8"),
);
const CSP: string = conf.app.security.csp;

/** Compiles the served file on a page under `csp`, and says how it went. */
async function compileUnder(page: Page, csp: string) {
  await page.route("**/csp-check.html", (route) =>
    route.fulfill({
      contentType: "text/html",
      headers: { "content-security-policy": csp },
      body: '<!doctype html><title>checking</title><script src="/csp-check.js"></script>',
    }),
  );
  await page.route("**/csp-check.js", (route) =>
    route.fulfill({
      contentType: "text/javascript",
      body: `WebAssembly.compileStreaming(fetch("/ghostty/ghostty-vt.wasm")).then(
        (module) => { document.title = "compiled:" + WebAssembly.Module.exports(module).length; },
        (error) => { document.title = "refused:" + error.name; });`,
    }),
  );
  await page.goto("/csp-check.html");
  await expect(page).not.toHaveTitle("checking");
  return page.title();
}

test("the window's CSP lets it compile libghostty-vt", async ({ page }) => {
  expect(CSP).toContain("script-src 'self' 'wasm-unsafe-eval';");
  expect(await compileUnder(page, CSP)).toMatch(/^compiled:\d+$/);
});

test("without 'wasm-unsafe-eval' the CSP refuses it", async ({ page }) => {
  expect(await compileUnder(page, CSP.replace(" 'wasm-unsafe-eval'", ""))).toMatch(/^refused:/);
});
