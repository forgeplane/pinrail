import { defineConfig } from "@playwright/test";

// One real server, one real CLI, one real browser, shared by every test.
// Tests create their own gates, so they never interfere, but they run on one
// worker because a few of them (restart, inbox counts) reason about global
// state.
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
