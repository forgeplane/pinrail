// Runs the shell suite: builds the core, the CLI and the SDK copy once, then
// runs Playwright with a worker for every two CPUs, each against a core of
// its own (see shell.config.ts and shell/global-setup.ts).
//
//   node run-shell.mjs                         every spec
//   SHELL_WORKERS=3 node run-shell.mjs         with three workers
//   node run-shell.mjs work-items -g "queue"   arguments go to Playwright

import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { runScript } from "../scripts/sh.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, "..");
const started = Date.now();
const seconds = () => ((Date.now() - started) / 1000).toFixed(0);

function run(command, args, cwd, env = process.env) {
  return spawnSync(command, args, { cwd, env, stdio: "inherit", shell: process.platform === "win32" }).status;
}

function build(command, args, cwd) {
  if (run(command, args, cwd) !== 0) {
    console.error(`run-shell: ${command} ${args.join(" ")} failed`);
    process.exit(1);
  }
}

// CI builds from the lock file as it is
const locked = process.env.CI ? ["--locked"] : [];
build("cargo", ["build", "-q", "-p", "pinrail-desktop", ...locked], path.join(root, "desktop"));
build("cargo", ["build", "-q", ...locked], path.join(root, "cli"));
build("npm", ["run", "sdk:build", "--silent"], path.join(root, "desktop", "app"));
if (runScript(path.join(root, "scripts", "ghostty-wasm.sh"), [], { cwd: root }) !== 0) {
  console.error("run-shell: scripts/ghostty-wasm.sh failed");
  process.exit(1);
}
console.log(`run-shell: built in ${seconds()}s`);

const status = run("npx", ["playwright", "test", "-c", "shell.config.ts", ...process.argv.slice(2)], here, {
  ...process.env,
  SHELL_SDK_BUILT: "1",
  SHELL_BUILT: "1",
});
console.log(`run-shell: done in ${seconds()}s`);
process.exit(status ?? 1);
