import { expect, test } from "@playwright/test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { gateFrom, mountPlugin } from "@forgeplane/pinrail-plugin/testing";

// The harness holds a plugin to what the app would: a decision that does not
// pass the plugin's own decision_schema is refused there, so it fails here.

/** A plugin whose view hands over `{ ok: "yes" }`, a string where its schema wants a boolean. */
function wrongDecision(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pinrail-harness-"));
  fs.mkdirSync(path.join(dir, "schemas"));
  fs.writeFileSync(
    path.join(dir, "schemas", "decision.schema.json"),
    JSON.stringify({ type: "object", required: ["ok"], properties: { ok: { type: "boolean" } } }),
  );
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({ name: "wrong", version: 1, entry: "index.html", payload_schema: {}, decision_schema: { $ref: "schemas/decision.schema.json" } }),
  );
  fs.writeFileSync(
    path.join(dir, "index.html"),
    `<!doctype html>
<meta charset="utf-8">
<script src="/sdk/v1/pinrail-plugin.js"></script>
<script>const plugin = Pinrail.connect({ onCollect() { plugin.submit({ ok: "yes" }); } });</script>`,
  );
  return dir;
}

test("a decision that fails the plugin's decision_schema fails nextSubmit", async ({ page }) => {
  const plugin = await mountPlugin(page, wrongDecision(), { gate: gateFrom({ title: "Wrong", payload: {} }) });
  await plugin.collect();
  await expect(plugin.nextSubmit()).rejects.toThrow("does not pass decision_schema: /ok: must be boolean");
  // a test about a refusal can still read it
  expect(await plugin.nextSubmit(0, { valid: false })).toEqual({ ok: "yes" });
});
