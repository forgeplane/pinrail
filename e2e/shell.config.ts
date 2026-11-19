import { defineConfig } from "@playwright/test";
import os from "node:os";
import path from "node:path";
import { uiPort } from "./shell/helpers";

// The desktop shell in a browser: a production build of the app's UI against
// headless desktop cores on scratch data folders, one core for each worker,
// which global-setup.ts starts and fixtures.ts gives each page.
//
// SHELL_WORKERS sets how many workers, else one for every two CPUs.
// SHELL_SDK_BUILT says the SDK copy and the terminal's WebAssembly are built,
// and SHELL_BUILT that the core and the CLI are: run-shell.mjs builds them
// first and sets both.
const root = path.resolve(__dirname, "..");
const feedbackUrl = "https://feedback.test/v1/feedback";
const ui = path.join(__dirname, ".state", "shell-ui");
const workers = Number(process.env.SHELL_WORKERS) || Math.max(1, Math.floor(os.availableParallelism() / 2));

export default defineConfig({
  testDir: path.join(__dirname, "shell"),
  // after the web server: a core for each worker
  globalSetup: "./shell/global-setup.ts",
  // every test stands alone, so the workers split the tests one by one
  // rather than file by file
  workers,
  // a test that fails once in CI runs again, and the report marks it flaky
  retries: process.env.CI ? 1 : 0,
  fullyParallel: true,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  reporter: [["list"]],
  use: {
    baseURL: `http://127.0.0.1:${uiPort}`,
    headless: true,
    // recording a trace slows every test, so CI records one only when a
    // test runs again after a failure, and a local run with --trace
    trace: process.env.CI ? "on-first-retry" : "off",
    // the app's endless animations stop, as they do for a person who asks
    // for less motion, so they do not keep the browsers of every worker busy
    contextOptions: { reducedMotion: "reduce" },
  },
  webServer: [
    {
      // a production build of the UI, served as files: far lighter than the
      // dev server, which compiles every module again for each page a test
      // opens
      command: `${process.env.SHELL_SDK_BUILT ? "" : "npm run sdk:build && npm run ghostty:wasm && "}npx vite build --outDir "${ui}" --emptyOutDir --logLevel error && npx vite preview --outDir "${ui}" --host 127.0.0.1 --port ${uiPort} --strictPort`,
      cwd: path.join(root, "desktop", "app"),
      // feedback goes to an address the tests answer, never to the service
      env: { VITE_PINRAIL_FEEDBACK_URL: feedbackUrl },
      url: `http://127.0.0.1:${uiPort}`,
      reuseExistingServer: false,
      timeout: 120_000,
      stdout: "ignore",
      stderr: "pipe",
    },
  ],
});
