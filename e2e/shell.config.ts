import { defineConfig } from "@playwright/test";
import path from "node:path";

// The desktop shell in a browser: the app's UI from vite against the
// desktop core running headless on a scratch data directory. No CLI. The
// core is built first when it is out of date, which takes a while once.
// The data starts as a returning person's: the setup already seen, so its
// dialog does not cover what the tests click.
const root = path.resolve(__dirname, "..");
const corePort = 4799;
const uiPort = 5199;
const data = path.join(__dirname, ".state", "shell-data");

export default defineConfig({
  testDir: path.join(__dirname, "shell"),
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
      command: `cargo build -q -p pinrail-desktop && rm -rf "${data}" && mkdir -p "${data}" && echo '{"welcome":{"seen":true}}' > "${data}/settings.json" && ./target/debug/Pinrail --headless --port ${corePort} --data-dir "${data}" --sdk-dir "${path.join(root, "desktop", "app", "sdk", "v1")}"`,
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
      env: { VITE_PINRAIL_URL: `http://127.0.0.1:${corePort}` },
      url: `http://127.0.0.1:${uiPort}`,
      reuseExistingServer: false,
      timeout: 120_000,
      stdout: "ignore",
      stderr: "pipe",
    },
  ],
});
