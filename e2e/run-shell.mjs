// Runs the shell suite in shards side by side, each with its own core, Vite
// and browser on ports and in folders of its own (see shell.config.ts). The
// core, the CLI and the SDK copy are built once first, so the shards neither
// wait on cargo's lock nor write the copy at once.
//
//   node run-shell.mjs                  every spec, in one shard for every two CPUs
//   SHELL_SHARDS=3 node run-shell.mjs   in three
//   node run-shell.mjs work-items -g "queue"   arguments go to each shard
//
// Each shard's log is printed whole once it ends; a failed one is printed
// last, so its failures are what the terminal shows.

import { spawn, spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, "..");
const args = process.argv.slice(2);
const shards = Math.max(1, Number(process.env.SHELL_SHARDS) || Math.floor(os.availableParallelism() / 2));
const started = Date.now();
const seconds = () => ((Date.now() - started) / 1000).toFixed(0);

function build(command, commandArgs, cwd) {
  const result = spawnSync(command, commandArgs, { cwd, stdio: "inherit", shell: process.platform === "win32" });
  if (result.status !== 0) {
    console.error(`run-shell: ${command} ${commandArgs.join(" ")} failed`);
    process.exit(result.status ?? 1);
  }
}

// CI builds from the lock file as it is
const locked = process.env.CI ? ["--locked"] : [];
build("cargo", ["build", "-q", "-p", "pinrail-desktop", ...locked], path.join(root, "desktop"));
build("cargo", ["build", "-q", ...locked], path.join(root, "cli"));
build("npm", ["run", "sdk:build", "--silent"], path.join(root, "desktop", "app"));
console.log(`run-shell: built in ${seconds()}s; running ${shards} shard${shards === 1 ? "" : "s"}`);

const runs = Array.from({ length: shards }, (_, i) => {
  const n = i + 1;
  return new Promise((resolve) => {
    let log = "";
    const child = spawn(
      "npx",
      [
        "playwright",
        "test",
        "-c",
        "shell.config.ts",
        `--shard=${n}/${shards}`,
        `--output=test-results/shard-${n}`,
        ...args,
      ],
      {
        cwd: here,
        env: {
          ...process.env,
          SHELL_SHARD: String(n),
          SHELL_SDK_BUILT: "1",
          SHELL_BUILT: "1",
          // a transform cache of its own: on Windows every process shares
          // one by default, and shards writing the same file at once fail
          // with EPERM
          PWTEST_CACHE_DIR: path.join(os.tmpdir(), `pinrail-shell-transform-cache-${n}`),
        },
        shell: process.platform === "win32",
      },
    );
    child.stdout.on("data", (d) => (log += d));
    child.stderr.on("data", (d) => (log += d));
    child.on("close", (code) => resolve({ n, code: code ?? 1, log }));
  });
});

const results = await Promise.all(runs);
const failed = results.filter((r) => r.code !== 0);
for (const r of [...results.filter((r) => r.code === 0), ...failed]) {
  console.log(`\n── shard ${r.n} of ${shards} ${r.code === 0 ? "passed" : "FAILED"} ──`);
  process.stdout.write(r.log);
}
// the counts of every shard together
const count = (word) =>
  results.reduce((sum, r) => sum + Number(r.log.match(new RegExp(`(\\d+) ${word}`))?.[1] ?? 0), 0);
console.log(
  `\nrun-shell: ${count("passed")} passed, ${count("failed")} failed, ${count("flaky")} flaky, ` +
    `${count("skipped")} skipped, in ${seconds()}s`,
);
process.exit(failed.length ? 1 : 0);
