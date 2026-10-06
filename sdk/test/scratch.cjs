// A fresh folder in the system's temp directory, removed when the process
// running the tests exits. Outside the package, so nothing in it resolves
// modules from this package's node_modules.
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const made = [];
process.on("exit", () => {
  for (const dir of made) fs.rmSync(dir, { recursive: true, force: true });
});

function scratch(prefix) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  made.push(dir);
  return dir;
}

module.exports = { scratch };
