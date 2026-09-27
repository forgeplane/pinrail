// Sets the version everywhere it is written, before a release is tagged.
//
//   node scripts/set-version.mjs 0.2.0
//   node scripts/set-version.mjs 0.2.0 --tag   # also commits and tags v0.2.0
//
// Pushing is yours: `git push --follow-tags origin main` starts the release.

import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const run = (cmd, args, cwd = root) => execFileSync(cmd, args, { cwd, stdio: "inherit" });

const version = (process.argv[2] ?? "").replace(/^v/, "");
const alsoTag = process.argv.includes("--tag");
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error(`set-version: "${process.argv[2] ?? ""}" is not a version, e.g. 0.2.0 or 0.2.0-beta.1`);
  process.exit(1);
}

// what a release commit holds: the files this script writes, the lock files
// cargo rewrites, and the changelog written for it beforehand
const released = [
  "desktop/Cargo.toml",
  "desktop/Cargo.lock",
  "cli/Cargo.toml",
  "cli/Cargo.lock",
  "desktop/app/src-tauri/tauri.conf.json",
  "desktop/app/package.json",
  "CHANGELOG.md",
];

if (alsoTag) {
  // anything else changed would be tagged and built with the release
  const others = execFileSync("git", ["status", "--porcelain", "--untracked-files=no"], { cwd: root, encoding: "utf8" })
    .split("\n")
    .filter(Boolean)
    .map((line) => line.slice(3))
    .filter((file) => !released.includes(file));
  if (others.length) {
    console.error(`set-version: commit or stash these first, or they go into the release:\n  ${others.join("\n  ")}`);
    process.exit(1);
  }
}

// Sets the version in one file. A file that already has this version stays
// as it is, which is the case for the first release. A file whose version
// line cannot be found stops the script.
const write = (file, pattern, replacement) => {
  const full = path.join(root, file);
  const before = fs.readFileSync(full, "utf8");
  const found = before.match(pattern);
  if (!found) {
    console.error(`set-version: no version found in ${file}; has it moved?`);
    process.exit(1);
  }
  fs.writeFileSync(full, before.replace(pattern, replacement));
};

// the first `version = "…"` in a manifest is the package's own
write("desktop/Cargo.toml", /^version = "[^"]*"$/m, `version = "${version}"`);
write("cli/Cargo.toml", /^version = "[^"]*"$/m, `version = "${version}"`);
write("desktop/app/src-tauri/tauri.conf.json", /("version":\s*)"[^"]*"/, `$1"${version}"`);
write("desktop/app/package.json", /("version":\s*)"[^"]*"/, `$1"${version}"`);

// cargo writes the lock files, so a release still builds --locked
run("cargo", ["update", "--workspace", "--offline", "--quiet"], path.join(root, "desktop"));
run("cargo", ["update", "--workspace", "--offline", "--quiet"], path.join(root, "cli"));
console.log(`version ${version}`);

if (alsoTag) {
  // a release with nothing to say about itself is a mistake, not a release
  run("node", [path.join(root, "scripts", "release-notes.mjs"), version]);
  // the first release changes no version, so there may be nothing to commit
  const changed = spawnSync("git", ["diff", "--quiet", "HEAD", "--", ...released], { cwd: root }).status !== 0;
  if (changed) run("git", ["commit", "-m", `Release v${version}`, "--", ...released]);
  run("git", ["tag", "-a", `v${version}`, "-m", `Pinrail ${version}`]);
  console.log(`tagged v${version}; push with: git push --follow-tags origin main`);
}
