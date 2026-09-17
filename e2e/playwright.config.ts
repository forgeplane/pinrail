import { defineConfig } from "@playwright/test";

// The CLI against the desktop app's server, run headless, shared by every
// test. Tests create their own reviews, so they never interfere, but they run
// on one worker because the restart spec takes the server down.
export default defineConfig({
  testDir: "./tests",
  globalSetup: "./helpers/global-setup.ts",
  globalTeardown: "./helpers/global-teardown.ts",
  workers: 1,
  fullyParallel: false,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  reporter: [["list"]],
  use: {
    headless: true,
    trace: "retain-on-failure",
  },
});
