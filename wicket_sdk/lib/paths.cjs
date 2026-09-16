// Where the package's files are, and where the icon set is: the pinned
// lucide-static release the package depends on, or, in a checkout of the
// repository, the copy the app serves.
const fs = require("node:fs");
const { createRequire } = require("node:module");
const path = require("node:path");

/** The package's root, from any file under it. */
function packageRoot(file) {
  let dir = path.dirname(file);
  while (!fs.existsSync(path.join(dir, "package.json"))) {
    const up = path.dirname(dir);
    if (up === dir) throw new Error(`no package.json above ${file}`);
    dir = up;
  }
  return dir;
}

/** The directory of icon SVGs, or null when none can be found. */
function iconsDir(root) {
  try {
    const pkg = createRequire(path.join(root, "package.json")).resolve("lucide-static/package.json");
    const dir = path.join(path.dirname(pkg), "icons");
    if (fs.existsSync(dir)) return dir;
  } catch {
    // not installed: a checkout may still have the app's copy
  }
  const app = path.resolve(root, "..", "desktop", "app", "sdk", "v1", "icons");
  return fs.existsSync(app) ? app : null;
}

module.exports = { packageRoot, iconsDir };
