// `pinrail-plugin dev` on a free port, for the tests that open it.
import { expect } from "@playwright/test";
import { spawn } from "node:child_process";
import net from "node:net";
import path from "node:path";

const bin = path.resolve(import.meta.dirname, "..", "bin", "pinrail-plugin.mjs");

const freePort = () =>
  new Promise<number>((resolve) => {
    const server = net.createServer();
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address() as net.AddressInfo;
      server.close(() => resolve(port));
    });
  });

/** Runs the development shell on `dir` until `use` is done with its address. */
export async function withDevShell(dir: string, use: (url: string) => Promise<void>) {
  const port = await freePort();
  const shell = spawn(process.execPath, [bin, "dev", dir, "--port", String(port), "--no-open"], { stdio: "pipe" });
  try {
    await expect
      .poll(async () => (await fetch(`http://127.0.0.1:${port}/dev/manifest`).catch(() => null))?.status)
      .toBe(200);
    await use(`http://127.0.0.1:${port}/`);
  } finally {
    shell.kill();
  }
}
