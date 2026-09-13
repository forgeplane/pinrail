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
  const cli = process.env.WICKET_CLI ?? path.join(root, "cli", "target", "debug", "wicket");
  if (!process.env.WICKET_CLI) {
    console.log("e2e: building the CLI");
    execFileSync("cargo", ["build", "--quiet"], { cwd: path.join(root, "cli"), stdio: "inherit" });
  }
  if (!fs.existsSync(cli)) throw new Error(`CLI binary not found at ${cli}`);
  return cli;
}

function buildDesktop(): string | undefined {
  if (process.env.WICKET_E2E_SERVER !== "desktop") return undefined;
  const bin = process.env.WICKET_DESKTOP_BIN ?? path.join(root, "desktop", "target", "debug", "Wicket");
  if (!process.env.WICKET_DESKTOP_BIN) {
    console.log("e2e: building the desktop app");
    execFileSync("cargo", ["build", "--quiet", "-p", "wicket-desktop"], { cwd: path.join(root, "desktop"), stdio: "inherit" });
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
    serverDir: path.join(root, "server"),
    desktopBin,
  };
  fs.mkdirSync(state.dataDir, { recursive: true });
  fs.mkdirSync(state.configDir, { recursive: true });
  saveState(state);

  console.log(`e2e: starting the server on ${state.url} (data in ${run})`);
  const serve = spawnSync(cli, ["serve"], { env: cliEnv(state), encoding: "utf8", timeout: 120_000 });
  if (serve.status !== 0) {
    throw new Error(`wicket serve failed (${serve.status}):\n${serve.stderr}\n${serve.stdout}`);
  }

  const add = spawnSync(cli, ["plugins", "add", path.join(root, "plugins")], { env: cliEnv(state), encoding: "utf8" });
  if (add.status !== 0) throw new Error(`wicket plugins add failed:\n${add.stderr}`);
}
