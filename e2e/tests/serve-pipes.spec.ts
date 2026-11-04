import { expect, test } from "@playwright/test";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import { cliEnv, loadState, stateDir } from "../helpers/state";

const freePort = () =>
  new Promise<number>((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address() as net.AddressInfo;
      srv.close(() => resolve(port));
    });
    srv.on("error", reject);
  });

// A program that runs `pinrail serve` and reads its output through pipes, as
// an agent's tool or a script does, gets the answer as soon as the server
// answers: the server it starts does not hold the pipes open while it runs.
test("pinrail serve returns at once when its output goes to pipes", async () => {
  test.setTimeout(90_000);
  const shared = loadState();
  const port = await freePort();
  const run = path.join(stateDir, `serve-pipes-${Date.now()}`);
  const state = { ...shared, port, url: `http://127.0.0.1:${port}`, dataDir: path.join(run, "data") };
  fs.mkdirSync(state.dataDir, { recursive: true });

  const started = Date.now();
  // stdin, stdout and stderr are pipes, spawnSync's default
  const serve = spawnSync(state.cli, ["serve"], { env: cliEnv(state), encoding: "utf8", timeout: 60_000 });
  const took = Date.now() - started;
  try {
    expect(serve.error, `${serve.stderr}`).toBeUndefined();
    expect(serve.status, serve.stderr).toBe(0);
    expect(JSON.parse(serve.stdout).url).toBe(state.url);
    expect(took, `pinrail serve took ${took} ms`).toBeLessThan(20_000);
  } finally {
    const info = path.join(state.dataDir, "server.json");
    const pid = fs.existsSync(info) ? JSON.parse(fs.readFileSync(info, "utf8")).pid : null;
    if (pid) process.kill(pid);
  }
});
