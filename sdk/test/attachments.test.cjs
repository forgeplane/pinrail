// The harness and the dev shell name a file's media type as the app does:
// the SDK's table is the format crate's, which the CLI and the app use.
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { TYPES } = require("../lib/attachments.cjs");

/** The extensions and media types in `media_type` of the format crate. */
function rustTypes() {
  const source = fs.readFileSync(
    path.join(__dirname, "..", "..", "desktop", "format", "src", "attachments.rs"),
    "utf8",
  );
  const body = source.slice(source.indexOf("pub fn media_type"));
  const types = {};
  for (const [, exts, type] of body.matchAll(/^\s+((?:"\w+"(?:\s*\|\s*)?)+)\s*=>\s*"([^"]+)",$/gm)) {
    for (const [, ext] of exts.matchAll(/"(\w+)"/g)) types[`.${ext}`] = type;
  }
  return types;
}

test("the media types are the app's", () => {
  const rust = rustTypes();
  assert.ok(Object.keys(rust).length > 20, "the format crate's table was read");
  assert.deepEqual(TYPES, rust);
});
