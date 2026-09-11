import { defineConfig } from "@playwright/test";
import path from "node:path";

// Isolated plugin tests: each plugin's tests/*.spec.ts, mounted under the
// SDK's fake shell. No server, no CLI; see wicket_sdk/testing.
export default defineConfig({
  testDir: path.resolve(__dirname, ".."),
  testMatch: /(plugins|server\/priv\/plugins)\/[^/]+\/tests\/.*\.spec\.ts$/,
  testIgnore: ["**/node_modules/**", "**/_build/**", "**/deps/**", "**/target/**"],
  timeout: 30_000,
  expect: { timeout: 5_000 },
  reporter: [["list"]],
  use: { headless: true, trace: "retain-on-failure" },
});
