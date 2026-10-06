// The Manifest type is written by hand beside the manifest schema, which
// the app and `check` hold manifests to: the two must define the same keys,
// or an author typing a manifest is told a valid key is an error, or is
// offered one the app refuses.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

test("the Manifest type has the keys the manifest schema defines", () => {
  const schema = JSON.parse(fs.readFileSync(path.join(__dirname, "..", "schemas", "manifest.schema.json"), "utf8"));
  const types = fs.readFileSync(path.join(__dirname, "..", "types.d.ts"), "utf8");
  const start = types.indexOf("export type Manifest = {");
  const body = types.slice(start, types.indexOf("\n};", start));
  // a top-level key is indented by two spaces, bare or quoted, optional or not
  const typed = [...body.matchAll(/^ {2}"?([\w$]+)"?\??:/gm)].map((m) => m[1]);
  assert.deepEqual(typed.sort(), Object.keys(schema.properties).sort());
});
