// The tests that need a real browser: the terminal renderers' contract,
// which draws on a canvas, measures fonts and runs WebAssembly. They run in
// WebKit, the engine of the app's window on macOS and Linux.
import path from "node:path";
import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

export default defineConfig({
  // what the app serves, such as libghostty-vt's WebAssembly, which
  // scripts/ghostty-wasm.sh builds there
  publicDir: "ui/public",
  resolve: {
    alias: {
      "pinrail-sdk/host": path.resolve(__dirname, "..", "..", "sdk", "host", "host.js"),
    },
  },
  test: {
    include: ["ui/src/**/*.browser.test.ts"],
    browser: {
      enabled: true,
      provider: playwright(),
      headless: true,
      instances: [{ browser: "webkit" }],
    },
  },
});
