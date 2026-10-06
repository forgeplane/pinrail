// Unit tests for the UI's pure logic: formatting, key handling, how events
// change the pending list. Components are tested end to end, in a real
// browser against the real core (e2e/shell), so these tests run in Node,
// with no DOM.
import path from "node:path";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "pinrail-sdk/host": path.resolve(__dirname, "..", "..", "pinrail-plugin", "host", "host.js"),
    },
  },
  test: {
    include: ["ui/src/**/*.test.ts"],
    environment: "node",
  },
});
