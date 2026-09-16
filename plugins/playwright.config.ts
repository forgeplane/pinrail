import { defineConfig } from "@playwright/test";
import path from "node:path";

// Each plugin's tests/*.spec.ts, mounted under the wicket-plugin harness:
// the samples here and the built-in list plugin inside the core. No server,
// no CLI. The built-in one sits outside this folder, which is why npm test
// puts node_modules on NODE_PATH: its spec resolves the package the same way.
export default defineConfig({
  testDir: path.resolve(__dirname, ".."),
  testMatch: /(plugins\/[^/]+|desktop\/core\/builtin\/list)\/tests\/.*\.spec\.ts$/,
  testIgnore: ["**/node_modules/**", "**/target/**"],
  timeout: 30_000,
  expect: { timeout: 5_000 },
  reporter: [["list"]],
  use: { headless: true, trace: "retain-on-failure" },
});
