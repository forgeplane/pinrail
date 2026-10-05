import { defineConfig } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { corePort } from "./shell/helpers";

// The desktop shell in a browser: the app's UI from vite against the
// desktop core running headless on a scratch data directory. The
// core is built first when it is out of date, which takes a while once, and
// the CLI with it, for the one test where an agent waits on a person.
// The data starts as a returning person's: the setup already seen, so its
// dialog does not cover what the tests click.
const root = path.resolve(__dirname, "..");
const uiPort = 5199;
const feedbackUrl = "https://feedback.test/v1/feedback";
const data = path.join(__dirname, ".state", "shell-data");

// An official plugin at two versions, which the app does not carry, so an
// installed older one is offered the newer: notes, made from hello.
const catalog = path.join(__dirname, ".state", "shell-catalog");
fs.rmSync(catalog, { recursive: true, force: true });
for (const version of ["1.0.0", "1.1.0"]) {
  const dir = path.join(catalog, `notes-${version}`);
  fs.cpSync(path.join(root, "plugins", "hello"), dir, {
    recursive: true,
    filter: (src) => !/\/(tests|fixtures|node_modules)(\/|$)/.test(src),
  });
  const manifest = JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8"));
  fs.writeFileSync(
    path.join(dir, "manifest.json"),
    JSON.stringify({ ...manifest, name: "notes", title: "Notes", version }),
  );
}

export default defineConfig({
  testDir: path.join(__dirname, "shell"),
  // after the web servers: the official plugins the specs use
  globalSetup: "./shell/global-setup.ts",
  workers: 1,
  fullyParallel: false,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  reporter: [["list"]],
  use: {
    baseURL: `http://127.0.0.1:${uiPort}`,
    headless: true,
    trace: "retain-on-failure",
  },
  webServer: [
    {
      command: `cargo build -q -p pinrail-desktop && cargo build -q --manifest-path ../cli/Cargo.toml && rm -rf "${data}" && mkdir -p "${data}" && echo '{"welcome":{"seen":true}}' > "${data}/settings.json" && ./target/debug/Pinrail --headless --port ${corePort} --data-dir "${data}" --sdk-dir "${path.join(root, "desktop", "app", "sdk", "v1")}" --catalog-dir "${catalog}"`,
      cwd: path.join(root, "desktop"),
      env: { PINRAIL_SHELL_ORIGIN: `http://127.0.0.1:${uiPort}` },
      url: `http://127.0.0.1:${corePort}/api/v1/info`,
      reuseExistingServer: false,
      timeout: 600_000,
      stdout: "ignore",
      stderr: "pipe",
    },
    {
      command: `npm run sdk:build && npx vite --host 127.0.0.1 --port ${uiPort} --strictPort`,
      cwd: path.join(root, "desktop", "app"),
      // feedback goes to an address the tests answer, never to the service
      env: { VITE_PINRAIL_URL: `http://127.0.0.1:${corePort}`, VITE_PINRAIL_FEEDBACK_URL: feedbackUrl },
      url: `http://127.0.0.1:${uiPort}`,
      reuseExistingServer: false,
      timeout: 120_000,
      stdout: "ignore",
      stderr: "pipe",
    },
  ],
});
