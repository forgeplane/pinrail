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
// HOME too, so this takes it away. The data is in the profile's local app
// data there, which the test moves into its own profile too.
test("without HOME, the server keeps its data in the USERPROFILE folder", async () => {
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), "pinrail-profile-"));
  const local = path.join(profile, "AppData", "Local");
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    USERPROFILE: profile,
    LOCALAPPDATA: local,
    // a terminal host that is never there, as in the shell suite: a real
    // one would outlive the server and, on Windows, keep the profile from
    // being removed while its program runs from it
    PINRAIL_HOST_SOCKET: path.join(profile, "no-host.sock"),
    PINRAIL_HOST_SECRET: "none",
  };
  for (const name of ["HOME", "PINRAIL_DATA_DIR", "XDG_DATA_HOME"]) delete env[name];
  const port = await freePort();
  const server = spawn(loadState().desktopBin, ["--headless", "--port", String(port)], { env, stdio: "ignore" });
  const exited = new Promise((resolve) => server.once("exit", resolve));
  try {
    const info =
      process.platform === "win32"
        ? path.join(local, "dev.pinrail.desktop", "server.json")
        : path.join(profile, ".local", "share", "pinrail", "server.json");
    await expect.poll(() => fs.existsSync(info), { timeout: 30_000 }).toBe(true);
  } finally {
    server.kill();
    await exited;
    fs.rmSync(profile, { recursive: true, force: true });
  }
});
