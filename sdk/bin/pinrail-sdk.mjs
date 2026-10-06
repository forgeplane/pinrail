#!/usr/bin/env node
// The plugin author's command: one word, then the rest of the line.
//
//   pinrail-sdk dev [dir] [--port N] [--no-open]        the fake shell in a browser, reloading on change
//
// A plugin is created, and checked, by the `pinrail` command; its tests run
// with `playwright test`, under the harness this package exports.
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const [command, ...rest] = process.argv.slice(2);

const usage = `usage: pinrail-sdk <command> [options]

  dev [dir]       the fake shell in a browser, serving the plugin in dir (default .), reloading on change
                  --port N (default 4790), --no-open

A new plugin:      pinrail plugins new <name> [--template plain|vite|react] [--playwright]
What the app says: pinrail plugins check <dir>
In the app:        pinrail plugins install <dir> --link`;

switch (command) {
  case "dev": {
    const { serve } = await import(pathToFileURL(path.join(here, "..", "shell", "serve.mjs")).href);
    serve(rest);
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
