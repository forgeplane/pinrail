#!/usr/bin/env node
// The plugin author's command: one word, then the rest of the line.
//
//   wicket-plugin create <name> [--template plain|vite]   a plugin folder to start from
//   wicket-plugin dev [dir] [--port N] [--no-open]        the fake shell in a browser, reloading on change
import { fileURLToPath } from "node:url";
import path from "node:path";
import { pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const [command, ...rest] = process.argv.slice(2);

const usage = `usage: wicket-plugin <command> [options]

  create <name>   a plugin folder to start from: manifest, schemas, view, fixture, test, release workflow
                  --template plain|vite (default plain), --dir path (default ./<name>)
  dev [dir]       the fake shell in a browser, serving the plugin in dir (default .), reloading on change
                  --port N (default 4790), --no-open

Then, in the app:  wicket plugins install <dir> --link`;

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
