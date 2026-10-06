#!/usr/bin/env node
// The plugin author's command: one word, then the rest of the line.
//
//   pinrail-sdk dev [dir] [--port N] [--no-open]        the fake shell in a browser, reloading on change
//   pinrail-sdk test [dir] [playwright arguments]       the plugin's tests/ under the harness
//   pinrail-sdk check [dir] [--json]                    what the app would say, through `pinrail plugins check`
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const [command, ...rest] = process.argv.slice(2);

const usage = `usage: pinrail-sdk <command> [options]

  dev [dir]       the fake shell in a browser, serving the plugin in dir (default .), reloading on change
                  --port N (default 4790), --no-open
  test [dir]      the plugin's tests/ under the harness, with Playwright from its dependencies
                  anything else on the line goes to Playwright: -g "hands over", --headed
  check [dir]     what the app would say of the folder: problems that refuse it, warnings that cost a feature
                  --json

Then, in the app:  pinrail plugins install <dir> --link`;

switch (command) {
  case "dev": {
    const { serve } = await import(pathToFileURL(path.join(here, "..", "shell", "serve.mjs")).href);
    serve(rest);
    break;
  }
  case "test": {
    const { runTests } = await import(pathToFileURL(path.join(here, "..", "lib", "test.mjs")).href);
    runTests(rest);
    break;
  }
  case "check": {
    // the app's own rules, as the pinrail command runs them; one
    // implementation, not a copy of it here
    const ran = spawnSync("pinrail", ["plugins", "check", ...rest], { stdio: "inherit" });
    if (ran.error) {
      console.error(
        "pinrail-sdk check runs `pinrail plugins check`, and the pinrail command was not found. Install Pinrail and its command (Settings › Data › Command line), then try again.",
      );
      process.exit(1);
    }
    process.exit(ran.status ?? 1);
    break;
  }
  case undefined:
  case "-h":
  case "--help":
  case "help":
    console.log(usage);
    break;
  default:
    console.error(`unknown command: ${command}\n\n${usage}`);
    process.exit(2);
}
