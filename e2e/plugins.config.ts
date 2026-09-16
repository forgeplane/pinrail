import { defineConfig } from "@playwright/test";
import path from "node:path";

// Isolated plugin tests: each plugin's tests/*.spec.ts, mounted under the
// wicket-plugin harness, and sdk/ here, what `wicket-plugin create` writes
// under the same harness. No server, no CLI; see wicket-plugin/.
export default defineConfig({
  testDir: path.resolve(__dirname, ".."),
  testMatch: /((plugins\/[^/]+|desktop\/core\/builtin\/list)\/tests|e2e\/sdk)\/.*\.spec\.ts$/,
  testIgnore: ["**/node_modules/**", "**/_build/**", "**/deps/**", "**/target/**"],
  timeout: 30_000,
  expect: { timeout: 5_000 },
  reporter: [["list"]],
  use: { headless: true, trace: "retain-on-failure" },
});
