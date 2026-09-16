// `wicket-plugin test [dir] [playwright args]`: the plugin's tests/ under
// the harness. Playwright comes from the plugin's own dependencies (or the
// folder this runs in); the config is the plugin's playwright.config when it
// has one, the package's otherwise.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { packageRoot } from "./paths.cjs";

const root = packageRoot(fileURLToPath(import.meta.url));

/** The Playwright CLI script, looked up from the plugin, this folder, then the package. */
function playwrightCli(dir) {
  for (const from of [path.join(dir, "package.json"), path.join(process.cwd(), "package.json"), path.join(root, "package.json")]) {
    const req = createRequire(from);
    for (const id of ["@playwright/test/cli.js", "@playwright/test/package.json"]) {
      try {
        return path.join(path.dirname(req.resolve(id)), "cli.js");
      } catch {
        // not from there, or not that way
      }
    }
  }
  return null;
}

/** The command line: `test [dir] [playwright arguments]`; the dir is the first argument naming a plugin. */
export function runTests(argv) {
  const rest = [...argv];
  let dir = ".";
  const i = rest.findIndex((a) => !a.startsWith("-"));
  if (i >= 0 && fs.existsSync(path.join(rest[i], "manifest.json"))) dir = rest.splice(i, 1)[0];
  dir = path.resolve(dir);

  if (!fs.existsSync(path.join(dir, "manifest.json"))) {
    console.error(`no manifest.json in ${dir}\nusage: wicket-plugin test [dir] [playwright arguments]`);
    process.exit(2);
  }
  if (!fs.existsSync(path.join(dir, "tests"))) {
    console.error(`no tests/ in ${dir}: wicket-plugin create writes one to start from`);
    process.exit(2);
  }
  const cli = playwrightCli(dir);
  if (!cli) {
    console.error("@playwright/test is not installed: npm install --save-dev @playwright/test && npx playwright install chromium");
    process.exit(2);
  }

  const own = ["playwright.config.ts", "playwright.config.mts", "playwright.config.cts", "playwright.config.js", "playwright.config.mjs", "playwright.config.cjs"]
    .map((f) => path.join(dir, f))
    .find((f) => fs.existsSync(f));
  const config = own ?? path.join(root, "harness", "playwright.config.cjs");

  const r = spawnSync(process.execPath, [cli, "test", "-c", config, ...rest], {
    cwd: dir,
    stdio: "inherit",
    env: { ...process.env, WICKET_PLUGIN_DIR: dir },
  });
  process.exit(r.status ?? 1);
}
