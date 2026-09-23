#!/usr/bin/env node
// The plugin author's command: one word, then the rest of the line.
//
//   pinrail-plugin create <name> [--template plain|vite|react]   a plugin folder to start from
//   pinrail-plugin dev [dir] [--port N] [--no-open]        the fake shell in a browser, reloading on change
//   pinrail-plugin test [dir] [playwright arguments]       the plugin's tests/ under the harness
//   pinrail-plugin check [dir] [--json]                    what the app's inspect would say
import { fileURLToPath } from "node:url";
import path from "node:path";
import { pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const [command, ...rest] = process.argv.slice(2);

const usage = `usage: pinrail-plugin <command> [options]

  create <name>   a plugin folder to start from: manifest, schemas, view, fixture, test, release workflow
                  --template plain|vite|react (default plain), --dir path (default ./<name>)
  dev [dir]       the fake shell in a browser, serving the plugin in dir (default .), reloading on change
                  --port N (default 4790), --no-open
  test [dir]      the plugin's tests/ under the harness, with Playwright from its dependencies
                  anything else on the line goes to Playwright: -g "hands over", --headed
  check [dir]     what the app would say of the folder: problems that refuse it, warnings that cost a feature
                  --json

Then, in the app:  pinrail plugins install <dir> --link`;

switch (command) {
  case "create": {
    const { create } = await import(pathToFileURL(path.join(here, "..", "lib", "create.mjs")).href);
    create(rest);
    break;
  }
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
    const { check } = await import(pathToFileURL(path.join(here, "..", "lib", "check.mjs")).href);
    check(rest);
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
