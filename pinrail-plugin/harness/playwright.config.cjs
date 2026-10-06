// The configuration `pinrail-sdk test` uses for a plugin without one of
// its own: the plugin's tests/, headless, a trace kept for a failure.
const path = require("node:path");

const dir = process.env.PINRAIL_PLUGIN_DIR || process.cwd();

module.exports = {
  testDir: path.join(dir, "tests"),
  timeout: 30_000,
  expect: { timeout: 5_000 },
  reporter: [["list"]],
  use: { headless: true, trace: "retain-on-failure" },
};
