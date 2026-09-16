import { defineConfig } from "@playwright/test";
import path from "node:path";

// sdk/ here: what `wicket-plugin create` writes, under the harness. No
// server, no CLI. The plugins' own tests run from plugins/.
export default defineConfig({
  testDir: path.resolve(__dirname, "sdk"),
  testIgnore: ["**/node_modules/**", "**/target/**"],
  timeout: 30_000,
  expect: { timeout: 5_000 },
  reporter: [["list"]],
  use: { headless: true, trace: "retain-on-failure" },
});
