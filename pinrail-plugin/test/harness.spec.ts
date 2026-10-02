import { expect, test } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { reviewFrom, mountPlugin } from "@forgeplane/pinrail-plugin/testing";
import { scratch } from "./scratch.cjs";

// The harness holds a plugin to what the app would: a decision that does not
// pass the plugin's own decision_schema is refused there, so it fails here.

/** A plugin whose view hands over `{ ok: "yes" }`, a string where its schema wants a boolean. */
function wrongDecision(): string {
  const dir = scratch("pinrail-harness-");
  fs.mkdirSync(path.join(dir, "view"), { recursive: true });
  fs.mkdirSync(path.join(dir, "schemas"));
  fs.writeFileSync(
    path.join(dir, "schemas", "decision.schema.json"),
    JSON.stringify({ type: "object", required: ["ok"], properties: { ok: { type: "boolean" } } }),
  );
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({
      name: "wrong",
      version: "1.0.0",
    }),
  );
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html>
<meta charset="utf-8">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<script>const plugin = Pinrail.connect({ onCollect() { plugin.submit({ ok: "yes" }); } });</script>`,
  );
  return dir;
}

test("a decision that fails the plugin's decision_schema fails nextSubmit", async ({ page }) => {
  const plugin = await mountPlugin(page, wrongDecision(), { review: reviewFrom({ title: "Wrong", payload: {} }) });
  await plugin.collect();
  await expect(plugin.nextSubmit()).rejects.toThrow("does not pass decision_schema: /ok: must be boolean");
  // a test about a refusal can still read it
  expect(await plugin.nextSubmit(0, { valid: false })).toEqual({ ok: "yes" });
});

test("a view that loads its script by an absolute path fails here as in the app", async ({ page }) => {
  // the app serves a plugin's view/ under /bundles/<hash>/view/ and allows
  // scripts from there alone: /view.js is another server path, which it refuses
  const dir = scratch("pinrail-harness-");
  fs.mkdirSync(path.join(dir, "view"), { recursive: true });
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ name: "absolute", version: "1.0.0" }));
  fs.writeFileSync(path.join(dir, "view.js"), "document.documentElement.dataset.ran = 'yes';");
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html><meta charset="utf-8"><script src="/sdk/v1/pinrail-plugin.js"></script>
<script>Pinrail.connect({});</script><script src="/view.js"></script><p>view</p>`,
  );
  const plugin = await mountPlugin(page, dir, { review: reviewFrom({ title: "Absolute", payload: {} }) });
  await expect(plugin.frame.locator("p")).toHaveText("view");
  await page.waitForTimeout(300);
  expect(await plugin.frame.locator("html").getAttribute("data-ran"), "the absolute script ran").toBeNull();
});

test("sendKey sends only what the app would forward to the view", async ({ page }) => {
  // the app forwards a declared key, and not one it keeps for itself
  const dir = scratch("pinrail-harness-");
  fs.mkdirSync(path.join(dir, "view"), { recursive: true });
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({
      name: "keys",
      version: "1.0.0",
      shortcuts: [
        { keys: "j", does: "Next" },
        { keys: "cmd+enter", does: "Hand over" },
        { keys: "command+shift+f", does: "Fold" },
        { keys: "cmdorctrl+k", does: "Search" },
      ],
    }),
  );
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html><script src="/sdk/v1/pinrail-plugin.js"></script><script>Pinrail.connect({});</script><p>keys</p>`,
  );
  const plugin = await mountPlugin(page, dir, { review: reviewFrom({ title: "Keys", payload: {} }) });
  await plugin.sendKey("j");
  await expect(plugin.sendKey("x")).rejects.toThrow("not declared");
  await expect(plugin.sendKey("cmd+enter")).rejects.toThrow("the app keeps");
  // modifiers in any order or spelling are the same combination
  await plugin.sendKey("shift+cmd+f");
  // the app's own menu keys are never forwarded
  await expect(plugin.sendKey("cmdorctrl+k")).rejects.toThrow("the app keeps");
});

test("a view takes init only from the shell that holds its frame", async ({ page }) => {
  const dir = scratch("pinrail-harness-");
  fs.mkdirSync(path.join(dir, "view"), { recursive: true });
  fs.writeFileSync(path.join(dir, "manifest.json"), JSON.stringify({ name: "titled", version: "1.0.0" }));
  fs.writeFileSync(
    path.join(dir, "view", "index.html"),
    `<!doctype html>
<meta charset="utf-8">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<script>Pinrail.connect({ onInit({ review }) { document.body.dataset.title = review.title; } });</script>`,
  );
  const plugin = await mountPlugin(page, dir, { review: reviewFrom({ title: "Real", payload: {} }) });
  await expect(plugin.frame.locator("body")).toHaveAttribute("data-title", "Real");

  // another frame on the same page, claiming to be the shell
  await page.evaluate(() => {
    const sibling = document.createElement("iframe");
    sibling.srcdoc = `<script>
      parent.document.querySelector("iframe").contentWindow.postMessage(
        { pinrail: 1, type: "init", review: { title: "Forged", payload: {} }, shell_origin: parent.location.origin },
        "*",
      );
    </script>`;
    document.body.append(sibling);
  });
  await page.waitForTimeout(500);
  await expect(plugin.frame.locator("body")).toHaveAttribute("data-title", "Real");
});
