// Unit tests for the UI's pure logic: formatting, key handling, how events
// change the pending list. Components are tested end to end, in a real
// browser against the real core (e2e/shell), so these tests run in Node,
// with no DOM.
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["ui/src/**/*.test.ts"],
    environment: "node",
  },
});
