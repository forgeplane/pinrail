// Checks the relative links in the Markdown the repository tracks, outside
// the site and the test fixtures, with lychee, offline.
//
//   node scripts/links.mjs

import { spawnSync } from "node:child_process";

const listed = spawnSync("git", ["ls-files", "-z", "*.md", ":!docs", ":!website", ":!**/fixtures/**"], {
  encoding: "utf8",
});
if (listed.status !== 0) process.exit(listed.status ?? 1);
const files = listed.stdout.split("\0").filter(Boolean);
const run = spawnSync("lychee", ["--offline", "--no-progress", "--include-fragments", ...files], { stdio: "inherit" });
if (run.error) console.error(`links: ${run.error.message}; install lychee`);
process.exit(run.status ?? 1);
