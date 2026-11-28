// Runs ESLint or Prettier over the files Git tracks, as `git ls-files |
// xargs` did, on every system: Windows' cmd has no xargs, and its command
// line holds 32,767 characters, so the files go in batches. The tools run
// from their own scripts in node_modules, with no shell between.
//
//   node scripts/tracked.mjs eslint
//   node scripts/tracked.mjs prettier --check

import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const CODE = ["*.js", "*.mjs", "*.cjs", "*.ts", "*.cts", "*.mts", "*.tsx"];
const TOOLS = {
  eslint: { script: "node_modules/eslint/bin/eslint.js", patterns: CODE },
  prettier: { script: "node_modules/prettier/bin/prettier.cjs", patterns: [...CODE, "*.css"] },
};
const BATCH = 200;

const [name, ...args] = process.argv.slice(2);
const tool = TOOLS[name];
if (!tool) {
  console.error(`usage: node scripts/tracked.mjs ${Object.keys(TOOLS).join("|")} [arguments]`);
  process.exit(2);
}
const listed = spawnSync("git", ["ls-files", "-z", ...tool.patterns], { cwd: root, encoding: "utf8" });
if (listed.status !== 0) process.exit(listed.status ?? 1);
const files = listed.stdout.split("\0").filter(Boolean);
let failed = false;
for (let i = 0; i < files.length; i += BATCH) {
  const run = spawnSync(process.execPath, [path.join(root, tool.script), ...args, ...files.slice(i, i + BATCH)], {
    cwd: root,
    stdio: "inherit",
  });
  if (run.status !== 0) failed = true;
}
process.exit(failed ? 1 : 0);
