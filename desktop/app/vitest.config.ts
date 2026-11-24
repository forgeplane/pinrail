// Unit tests for the UI's pure logic: formatting, key handling, how events
// change the pending list. They run in Node, with no DOM. Components are
// tested in a browser: against a fake core with vitest.browser.config.ts
// (ui/src/testing/), and end to end against the real core (e2e/shell).
import path from "node:path";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "pinrail-sdk/host": path.resolve(__dirname, "..", "..", "sdk", "host", "host.js"),
    },
  },
  test: {
    // and the checks of what the app ships, beside its build scripts
    // and Pinrail's plugins for agents, which run in Node too
    include: [
      "ui/src/**/*.test.ts",
      "scripts/**/*.test.mjs",
      "ui/bench/**/*.test.mjs",
      "../runtime/src/agents/*.test.mjs",
    ],
    // those run in a browser, with vitest.browser.config.ts
    exclude: ["ui/src/**/*.browser.test.ts"],
    environment: "node",
  },
});
