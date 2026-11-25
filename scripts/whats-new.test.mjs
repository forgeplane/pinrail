// Tests pruning What's new to the last few releases, in a temporary folder.

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { notesDir, prune, released } from "./whats-new.mjs";

function notes(versions) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "whats-new-"));
  const dir = notesDir(root);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, "README.md"), "");
  for (const v of versions) {
    fs.writeFileSync(path.join(dir, `${v}.json`), "{}");
    fs.mkdirSync(path.join(dir, v));
    fs.writeFileSync(path.join(dir, v, "a-light.webp"), "");
  }
  return { root, left: () => fs.readdirSync(dir).sort() };
}

test("a release keeps the notes of the newest three releases up to it, and removes the rest", () => {
  const n = notes(["0.3.0", "0.4.0", "0.10.0", "0.5.0", "0.9.0"]);
  const removed = prune(n.root, "0.10.0");
  assert.deepEqual(n.left(), ["0.10.0", "0.10.0.json", "0.5.0", "0.5.0.json", "0.9.0", "0.9.0.json", "README.md"]);
  const base = path.join("desktop", "app", "ui", "src", "whats-new");
  assert.deepEqual(
    removed.sort(),
    [
      path.join(base, "0.3.0", "a-light.webp"),
      path.join(base, "0.3.0.json"),
      path.join(base, "0.4.0", "a-light.webp"),
      path.join(base, "0.4.0.json"),
    ].sort(),
  );
});

test("notes written for a later release are kept and do not count", () => {
  const n = notes(["0.3.0", "0.4.0", "0.5.0", "0.6.0"]);
  assert.deepEqual(prune(n.root, "0.5.0").length, 0);
  assert.ok(n.left().includes("0.6.0.json"));
});

test("the notes of the app's version and earlier are released, and later ones are not", () => {
  const n = notes(["0.5.0", "0.6.0", "0.7.0", "0.10.0"]);
  assert.deepEqual(released(n.root, "0.6.0").sort(), ["0.5.0", "0.6.0"]);
  assert.deepEqual(released(n.root, "0.9.0").sort(), ["0.5.0", "0.6.0", "0.7.0"]);
});
