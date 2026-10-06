import { defineConfig } from "@playwright/test";

// Each plugin's tests/*.spec.ts, mounted under the pinrail-sdk harness.
// No server, no CLI.
export default defineConfig({
  testDir: __dirname,
  testMatch: /[^/]+\/tests\/.*\.spec\.ts$/,
  testIgnore: ["**/node_modules/**", "**/target/**"],
  timeout: 30_000,
  expect: { timeout: 5_000 },
  reporter: [["list"]],
  use: { headless: true, trace: "retain-on-failure" },
});
