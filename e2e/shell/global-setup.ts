// Before the specs, once: a headless core for each worker, on a port and a
// scratch data folder of its own, with the official plugins the specs use
// installed, as a person would choose them, and the usage data notice
// answered. The UI is built once by the config's web server and given each
// worker's core when a page opens (see fixtures.ts).
//
// The core and the CLI are built first unless SHELL_BUILT says they are,
// which takes a while once; the CLI is for the one test where an agent
// waits on a person. The data starts as a returning person's: the setup
// already seen, so its dialog does not cover what the tests click.

import type { FullConfig } from "@playwright/test";
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { asPerson, corePortOf, personToken, shellOrigin } from "./helpers";

const root = path.resolve(__dirname, "..", "..");
const state = path.join(__dirname, "..", ".state");
const exe = process.platform === "win32" ? ".exe" : "";
const binary = path.join(root, "desktop", "target", "debug", `Pinrail${exe}`);

function run(command: string, args: string[], cwd: string) {
  const result = spawnSync(command, args, { cwd, stdio: "inherit", shell: process.platform === "win32" });
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed`);
}

/** An official plugin at two versions, which the app does not carry, so an
 *  installed older one is offered the newer: notes, made from hello. */
function writeCatalog(catalog: string) {
  fs.rmSync(catalog, { recursive: true, force: true });
  for (const version of ["1.0.0", "1.1.0"]) {
    const dir = path.join(catalog, `notes-${version}`);
    fs.cpSync(path.join(root, "plugins", "hello"), dir, {
      recursive: true,
      filter: (src) => !/[\\/](tests|fixtures|node_modules)([\\/]|$)/.test(src),
    });
    const manifest = JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8"));
    fs.writeFileSync(
      path.join(dir, "manifest.json"),
      JSON.stringify({ ...manifest, name: "notes", title: "Notes", version }),
    );
  }
}

/** Waits until the core of `port` answers, or fails with what it printed. */
async function answers(port: number, core: ChildProcess, log: string) {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    if (core.exitCode !== null) break;
    try {
      if ((await fetch(`http://127.0.0.1:${port}/api/v1/info`)).ok) return;
    } catch {
      // not listening yet
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`the core on port ${port} did not start:\n${fs.readFileSync(log, "utf8")}`);
}

/** Installs the plugins the specs use and answers the usage data notice. */
async function prepare(core: string) {
  for (const id of ["forgeplane/list", "forgeplane/feedback", "forgeplane/code-review"]) {
    const response = await fetch(`${core}/api/v1/plugins/install`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ id }),
    });
    if (!response.ok) throw new Error(`installing ${id}: ${response.status} ${await response.text()}`);
  }
  // as on a computer in use; its own spec brings the notice back
  const answered = await fetch(`${core}/api/v1/settings`, {
    method: "PATCH",
    headers: { "content-type": "application/json", ...asPerson },
    body: JSON.stringify({ telemetry: { notice_shown: "2026-01-01T00:00:00Z" } }),
  });
  if (!answered.ok) throw new Error(`answering the usage data notice: ${answered.status}`);
}

export default async function globalSetup(config: FullConfig) {
  if (!process.env.SHELL_BUILT) {
    run("cargo", ["build", "-q", "-p", "pinrail-desktop"], path.join(root, "desktop"));
    run("cargo", ["build", "-q"], path.join(root, "cli"));
  }
  fs.mkdirSync(state, { recursive: true });
  const tokenFile = path.join(state, "shell-token");
  fs.writeFileSync(tokenFile, personToken, { mode: 0o600 });
  const catalog = path.join(state, "shell-catalog");
  writeCatalog(catalog);

  const cores: ChildProcess[] = [];
  const stop = () => cores.forEach((core) => core.kill());
  try {
    await Promise.all(
      Array.from({ length: config.workers }, async (_, i) => {
        const n = i + 1;
        const port = corePortOf(n);
        const data = path.join(state, `shell-data-${n}`);
        run("node", [path.join(__dirname, "reset-data.mjs"), data], root);
        const log = path.join(state, `shell-core-${n}.log`);
        const out = fs.openSync(log, "w");
        const core = spawn(
          binary,
          [
            "--headless",
            "--port",
            String(port),
            "--data-dir",
            data,
            "--sdk-dir",
            path.join(root, "desktop", "app", "sdk", "v1"),
            "--catalog-dir",
            catalog,
            "--person-token-file",
            tokenFile,
          ],
          {
            cwd: path.join(root, "desktop"),
            stdio: ["ignore", "ignore", out],
            env: {
              ...process.env,
              PINRAIL_SHELL_ORIGIN: shellOrigin,
              // a terminal host that is never there: a spec that turns the
              // agent runtime on must not start a real host, which would
              // outlive it
              PINRAIL_HOST_SOCKET: path.join(data, "no-host.sock"),
              PINRAIL_HOST_SECRET: "none",
            },
          },
        );
        cores.push(core);
        await answers(port, core, log);
        await prepare(`http://127.0.0.1:${port}`);
      }),
    );
  } catch (error) {
    stop();
    throw error;
  }
  return stop;
}
