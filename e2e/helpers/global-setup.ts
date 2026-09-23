import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import { cliEnv, root, saveState, stateDir, type State } from "./state";

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address() as net.AddressInfo;
      srv.close(() => resolve(port));
    });
    srv.on("error", reject);
  });
}

function buildCli(): string {
  const cli = process.env.PINRAIL_CLI ?? path.join(root, "cli", "target", "debug", "pinrail");
  if (!process.env.PINRAIL_CLI) {
    console.log("e2e: building the CLI");
    execFileSync("cargo", ["build", "--quiet"], { cwd: path.join(root, "cli"), stdio: "inherit" });
  }
  if (!fs.existsSync(cli)) throw new Error(`CLI binary not found at ${cli}`);
  return cli;
}

/** The desktop app, which the CLI starts headless as the server. */
function buildDesktop(): string {
  const bin = process.env.PINRAIL_DESKTOP_BIN ?? path.join(root, "desktop", "target", "debug", "Pinrail");
  if (!process.env.PINRAIL_DESKTOP_BIN) {
    console.log("e2e: building the desktop app");
    execFileSync("cargo", ["build", "--quiet", "-p", "pinrail-desktop"], { cwd: path.join(root, "desktop"), stdio: "inherit" });
  }
  if (!fs.existsSync(bin)) throw new Error(`desktop binary not found at ${bin}`);
  return bin;
}

export default async function globalSetup() {
  const cli = buildCli();
  const desktopBin = buildDesktop();
  const port = await freePort();
  const run = path.join(stateDir, `run-${Date.now()}`);
  const state: State = {
    url: `http://127.0.0.1:${port}`,
    port,
    cli,
    dataDir: path.join(run, "data"),
    configDir: path.join(run, "config"),
    desktopBin,
  };
  fs.mkdirSync(state.dataDir, { recursive: true });
  fs.mkdirSync(state.configDir, { recursive: true });
  saveState(state);

  console.log(`e2e: starting the server on ${state.url} (data in ${run})`);
  const serve = spawnSync(cli, ["serve"], { env: cliEnv(state), encoding: "utf8", timeout: 120_000 });
  if (serve.status !== 0) {
    throw new Error(`pinrail serve failed (${serve.status}):\n${serve.stderr}\n${serve.stdout}`);
  }

  // One install per plugin, the way a person installs one. The three that
  // need no build are linked, so they are served from the folder they are
  // developed in; the artifact plugin is copied into the store, which runs
  // the build its manifest declares and is the only place that path is
  // exercised end to end. Its build fetches packages, so it gets longer.
  const install = (name: string, args: string[], timeout: number) => {
    const dir = path.join(root, "plugins", name);
    const done = spawnSync(cli, ["plugins", "install", dir, ...args], { env: cliEnv(state), encoding: "utf8", timeout });
    if (done.status !== 0) throw new Error(`pinrail plugins install ${name} failed:\n${done.stdout}\n${done.stderr}`);
  };
  for (const name of ["email", "hello", "review"]) install(name, ["--link"], 120_000);
  console.log("e2e: building and installing the artifact plugin");
  install("artifact", [], 900_000);
}
