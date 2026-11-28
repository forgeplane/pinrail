// The package as npm publishes it: every file its exports name is in the
// tarball, and the SDK's scripts run with nothing beside them.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

/** The packed package, extracted into a directory of its own. */
function packed() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pinrail-pack-"));
  // npm is a batch file on Windows, which runs only through a shell there
  const windows = process.platform === "win32";
  const out = execFileSync("npm", ["pack", "--json", "--pack-destination", windows ? `"${dir}"` : dir], {
    cwd: root,
    encoding: "utf8",
    shell: windows,
  });
  const [{ filename }] = JSON.parse(out.slice(out.indexOf("[")));
  // by its name in its folder: GNU tar, as Git Bash has, reads C: as a host
  execFileSync("tar", ["-xzf", filename], { cwd: dir });
  return path.join(dir, "package");
}

test("every export names a file the package ships, and the scripts work on their own", () => {
  const pkg = packed();
  const { exports } = JSON.parse(fs.readFileSync(path.join(pkg, "package.json"), "utf8"));
  const targets = Object.values(exports).flatMap((t) => (typeof t === "string" ? [t] : Object.values(t)));
  for (const target of targets) assert.ok(fs.existsSync(path.join(pkg, target)), `${target} is in the package`);

  // a page's globals that the parser uses
  const context = vm.createContext({ atob });
  for (const name of ["./sdk/v1/pinrail-plugin.js", "./sdk/v1/markdown.js", "./sdk/v1/highlight.js"]) {
    assert.ok(exports[name], `the package exports ${name}`);
    vm.runInContext(fs.readFileSync(path.join(pkg, exports[name]), "utf8"), context);
  }
  assert.equal(context.Pinrail.markdown("# Title"), "<h1>Title</h1>\n");
  assert.equal(context.Pinrail.languageOf("lib/acme.ex"), "elixir");
});
