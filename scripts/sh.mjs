// Runs a shell script with a POSIX sh on every system: the sh on the PATH,
// or on Windows, where a default Git install puts only Git's cmd folder on
// the PATH, the sh Git for Windows ships, which puts its own tools (sed,
// tar, sha256sum) on the PATH of the script.
//
//   node scripts/sh.mjs scripts/ghostty-wasm.sh [arguments]

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** The sh to run scripts with: a path, or "sh" to take it from the PATH. */
export function sh() {
  if (process.platform !== "win32") return "sh";
  const onPath = spawnSync("sh", ["-c", "exit 0"], { stdio: "ignore" });
  if (onPath.status === 0) return "sh";
  // Git's exec path is <Git>/mingw64/libexec/git-core, or the like for arm64
  const exec = spawnSync("git", ["--exec-path"], { encoding: "utf8" });
  const found = exec.status === 0 ? path.resolve(exec.stdout.trim(), "..", "..", "..", "bin", "sh.exe") : null;
  if (found && fs.existsSync(found)) return found;
  console.error("sh.mjs: no sh found; install Git for Windows, which brings one");
  process.exit(1);
}

/** Runs `script` with `args` and sh, and gives its exit status. */
export function runScript(script, args = [], options = {}) {
  return spawnSync(sh(), [script, ...args], { stdio: "inherit", ...options }).status ?? 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [script, ...args] = process.argv.slice(2);
  if (!script) {
    console.error("usage: node scripts/sh.mjs <script> [arguments]");
    process.exit(2);
  }
  process.exit(runScript(script, args));
}
