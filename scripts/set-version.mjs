// Sets the version everywhere it is written, before a release is tagged.
//
//   node scripts/set-version.mjs 0.2.0
//   node scripts/set-version.mjs 0.2.0 --tag   # also commits and tags v0.2.0
//
// Pushing is yours: `git push --follow-tags origin main` starts the release.

import { execFileSync } from "node:child_process";
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

const write = (file, change) => {
  const full = path.join(root, file);
  const before = fs.readFileSync(full, "utf8");
  const after = change(before);
  if (after === before) {
    console.error(`set-version: nothing to change in ${file}; has it moved?`);
    process.exit(1);
  }
  fs.writeFileSync(full, after);
};

// the first `version = "…"` in a manifest is the package's own
write("desktop/Cargo.toml", (t) => t.replace(/^version = "[^"]*"$/m, `version = "${version}"`));
write("cli/Cargo.toml", (t) => t.replace(/^version = "[^"]*"$/m, `version = "${version}"`));
write("desktop/app/src-tauri/tauri.conf.json", (t) => t.replace(/("version":\s*)"[^"]*"/, `$1"${version}"`));
write("desktop/app/package.json", (t) => t.replace(/("version":\s*)"[^"]*"/, `$1"${version}"`));

// cargo writes the lock files, so a release still builds --locked
run("cargo", ["update", "--workspace", "--offline", "--quiet"], path.join(root, "desktop"));
run("cargo", ["update", "--workspace", "--offline", "--quiet"], path.join(root, "cli"));
console.log(`version ${version}`);

if (alsoTag) {
  run("git", ["commit", "-am", `Release v${version}`]);
  run("git", ["tag", "-a", `v${version}`, "-m", `Wicket ${version}`]);
  console.log(`tagged v${version}; push with: git push --follow-tags origin main`);
}
