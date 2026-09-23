import { defineConfig } from "@playwright/test";

// The package's own Playwright spec: what `pinrail-plugin create` writes,
// mounted under the harness. The unit tests are Node's, in test/*.test.cjs.
export default defineConfig({
  testDir: "test",
  testMatch: /\.spec\.ts$/,
  timeout: 30_000,
  expect: { timeout: 5_000 },
  reporter: [["list"]],
  use: { headless: true, trace: "retain-on-failure" },
});
