const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { scratch } = require("./scratch.cjs");

const bin = path.resolve(__dirname, "..", "bin", "pinrail-plugin.mjs");

/** pinrail-plugin check with a pinrail command that prints its arguments and exits 2. */
function withPinrail(args, { pinrail = true } = {}) {
  const dir = scratch("pinrail-check-bin-");
  if (pinrail) {
    const fake = path.join(dir, "pinrail");
    fs.writeFileSync(fake, '#!/bin/sh\necho "args: $*"\nexit 2\n');
    fs.chmodSync(fake, 0o755);
  }
  return spawnSync(process.execPath, [bin, "check", ...args], {
    encoding: "utf8",
    env: { ...process.env, PATH: dir },
  });
}

test(
  "check runs pinrail plugins check with its arguments, and exits as it does",
  { skip: process.platform === "win32" },
  () => {
    const ran = withPinrail(["./plugin", "--json"]);
    assert.equal(ran.stdout, "args: plugins check ./plugin --json\n");
    assert.equal(ran.status, 2);
  },
);

test("check without the pinrail command says what it needs", () => {
  const ran = withPinrail([], { pinrail: false });
  assert.equal(ran.status, 1);
  assert.match(ran.stderr, /runs `pinrail plugins check`, and the pinrail command was not found/);
});
