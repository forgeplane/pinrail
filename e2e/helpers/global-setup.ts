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

export default async function globalSetup() {
  const cli = buildCli();
  const port = await freePort();
  const run = path.join(stateDir, `run-${Date.now()}`);
  const state: State = {
    url: `http://127.0.0.1:${port}`,
    port,
    cli,
    dataDir: path.join(run, "data"),
    configDir: path.join(run, "config"),
    serverDir: path.join(root, "server"),
  };
  fs.mkdirSync(state.dataDir, { recursive: true });
  fs.mkdirSync(state.configDir, { recursive: true });
  saveState(state);

  console.log(`e2e: starting the server on ${state.url} (data in ${run})`);
  const serve = spawnSync(cli, ["serve"], { env: cliEnv(state), encoding: "utf8", timeout: 120_000 });
  if (serve.status !== 0) {
    throw new Error(`wicket serve failed (${serve.status}):\n${serve.stderr}\n${serve.stdout}`);
  }

  const add = spawnSync(cli, ["types", "add", path.join(root, "plugins")], { env: cliEnv(state), encoding: "utf8" });
  if (add.status !== 0) throw new Error(`wicket types add failed:\n${add.stderr}`);
}
