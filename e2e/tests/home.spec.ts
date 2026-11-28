import { spawn } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { expect, test } from "@playwright/test";
import { loadState } from "../helpers/state";

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.once("error", reject);
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address() as net.AddressInfo;
      srv.close(() => resolve(port));
    });
  });
}

// Windows sets USERPROFILE and not HOME; Git Bash, which CI runs in, sets
// HOME too, so this takes it away. LOCALAPPDATA, which the server takes
// first on Windows, goes too: without either, the data is under the
// profile everywhere. A name is removed in any case, as Windows takes them.
test("without HOME, the server keeps its data in the USERPROFILE folder", async () => {
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), "pinrail-profile-"));
  const gone = ["HOME", "LOCALAPPDATA", "PINRAIL_DATA_DIR", "XDG_DATA_HOME"];
  const env: NodeJS.ProcessEnv = Object.fromEntries(
    Object.entries(process.env).filter(([name]) => !gone.includes(name.toUpperCase())),
  );
  // whatever the platform spelt it
  for (const name of Object.keys(env)) if (name.toUpperCase() === "USERPROFILE") delete env[name];
  Object.assign(env, {
    USERPROFILE: profile,
    // a terminal host that is never there, as in the shell suite: a real
    // one would outlive the server and, on Windows, keep the profile from
    // being removed while its program runs from it
    PINRAIL_HOST_SOCKET: path.join(profile, "no-host.sock"),
    PINRAIL_HOST_SECRET: "none",
  });
  const port = await freePort();
  const server = spawn(loadState().desktopBin, ["--headless", "--port", String(port)], { env, stdio: "ignore" });
  const exited = new Promise((resolve) => server.once("exit", resolve));
  try {
    const info = path.join(profile, ".local", "share", "pinrail", "server.json");
    await expect.poll(() => fs.existsSync(info), { timeout: 30_000 }).toBe(true);
  } finally {
    server.kill();
    await exited;
    fs.rmSync(profile, { recursive: true, force: true });
  }
});
