// The Manifest type is written by hand beside the manifest schema, which
// the app and `check` hold manifests to: every key the schema defines must
// be in the type, or an author typing a manifest is told a valid key is an
// error.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

test("the Manifest type has every key the manifest schema defines", () => {
  const schema = JSON.parse(fs.readFileSync(path.join(__dirname, "..", "schemas", "manifest.schema.json"), "utf8"));
  const types = fs.readFileSync(path.join(__dirname, "..", "types.d.ts"), "utf8");
  const start = types.indexOf("export type Manifest = {");
  const body = types.slice(start, types.indexOf("\n};", start));
  // a key is written bare or quoted, and optional or not
  const escaped = (key) => key.replace(/\$/g, "\\$");
  const missing = Object.keys(schema.properties).filter(
    (key) => !new RegExp(`^\\s*"?${escaped(key)}"?\\??:`, "m").test(body),
  );
  assert.deepEqual(missing, []);
});
