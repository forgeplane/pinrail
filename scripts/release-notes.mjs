// The changelog's section for one version, as the release notes.
//
//   node scripts/release-notes.mjs 0.1.0
//
// Exits 1 when the changelog says nothing about that version, so a release
// stops before it is drafted rather than going out with a heading and no
// account of what changed.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const version = (process.argv[2] ?? "").replace(/^v/, "");
if (!version) {
  console.error("release-notes: which version?");
  process.exit(1);
}

const changelog = fs.readFileSync(path.join(root, "CHANGELOG.md"), "utf8");
const heading = new RegExp(`^## \\[${version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\].*$`, "m");
const start = changelog.match(heading);
if (!start) {
  console.error(`release-notes: CHANGELOG.md has no section for ${version}`);
  process.exit(1);
}

const after = changelog.slice(start.index + start[0].length);
const next = after.search(/^## /m);
const notes = (next === -1 ? after : after.slice(0, next))
  // the link references at the foot belong to the file, not to the notes
  .replace(/^\[[^\]]+\]:.*$/gm, "")
  .trim();

if (!notes) {
  console.error(`release-notes: the section for ${version} is empty`);
  process.exit(1);
}
console.log(notes);
