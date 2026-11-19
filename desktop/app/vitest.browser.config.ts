// The tests that need a real browser: the terminal renderers' contract,
// which draws on a canvas, measures fonts and runs WebAssembly, and what
// reads the page's layout, such as find in page. They run in WebKit, the
// engine of the app's window on macOS and Linux.
import path from "node:path";
import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

export default defineConfig({
  // what the app serves, such as libghostty-vt's WebAssembly, which
  // scripts/ghostty-wasm.sh builds there
  publicDir: "ui/public",
  // bundled before the tests start: found while a test runs, Vite bundles
  // them then and reloads the page, which leaves two copies of React
  optimizeDeps: {
    include: ["react", "react/jsx-dev-runtime", "react-dom", "react-dom/client", "react-router", "lucide-react"],
  },
  resolve: {
    alias: {
      "pinrail-sdk/host": path.resolve(__dirname, "..", "..", "sdk", "host", "host.js"),
    },
  },
  test: {
    include: ["ui/src/**/*.browser.test.ts", "ui/src/**/*.browser.test.tsx"],
    // one file at a time: the files share the page's main thread, so one
    // file's work would delay another's frames and fail its timing checks
    fileParallelism: false,
    browser: {
      enabled: true,
      provider: playwright(),
      headless: true,
      instances: [{ browser: "webkit" }],
    },
  },
});
