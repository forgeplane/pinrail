import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests",
  timeout: 30_000,
  expect: { timeout: 5_000 },
  reporter: [["list"]],
  use: { headless: true, trace: "retain-on-failure" },
});
