// Where the package's files are.
const fs = require("node:fs");
const { createRequire } = require("node:module");
const path = require("node:path");

/** The package's root, from any file under it. */
function packageRoot(file) {
  let dir = path.dirname(file);
  while (!fs.existsSync(path.join(dir, "package.json"))) {
    const up = path.dirname(dir);
    if (up === dir) throw new Error(`no package.json above ${file}`);
    dir = up;
  }
  return dir;
}

/**
 * What the app serves at `/sdk/v1/pinrail-plugin.js`: the browser build of
 * markdown-it, then the SDK, which finds it as a global and configures it.
 * One script, so a view has markdown from its first line and asks the server
 * for nothing else. Assembled the same way here, in the app's build and in
 * the harness, so a view runs against one file wherever it runs.
 *
 * `from` is the package whose `node_modules` holds the parser; `src` the SDK
 * sources, which in a checkout of the app are not beneath it.
 */
function sdkScript(from, src = path.join(from, "src")) {
  const parser = createRequire(path.join(from, "package.json")).resolve("markdown-it/browser");
  // the parser's build names a source map that nothing serves; a browser's
  // devtools would ask for it, and log the 404, on every view
  const markdown = fs.readFileSync(parser, "utf8").replace(/\n?\/\/# sourceMappingURL=\S+\s*$/, "\n");
  return `${markdown}\n${fs.readFileSync(path.join(src, "pinrail-plugin.js"), "utf8")}`;
}

module.exports = { packageRoot, sdkScript };
