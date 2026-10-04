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
 * What the app serves at `/sdk/v1/markdown.js`: the browser build of
 * markdown-it, then the SDK's configuration of it in `src/markdown.js`, in a
 * scope of their own so the parser leaves no global behind. Built the same
 * way for the app, the harness, the development shell and the package, so a
 * view runs against one file wherever it runs.
 *
 * `from` is the package whose `node_modules` holds the parser; `src` the SDK
 * sources, which in a checkout of the app are not beneath it.
 */
function markdownScript(from, src = path.join(from, "src")) {
  const file = createRequire(path.join(from, "package.json")).resolve("markdown-it/browser");
  // the parser's build names a source map that nothing serves; a browser's
  // devtools would ask for it, and log the 404, on every view
  const parser = fs.readFileSync(file, "utf8").replace(/\n?\/\/# sourceMappingURL=\S+\s*$/, "\n");
  return [
    "(function () {",
    "var module = { exports: {} }, exports = module.exports;",
    parser,
    "var markdownit = module.exports;",
    fs.readFileSync(path.join(src, "markdown.js"), "utf8"),
    "})();",
    "",
  ].join("\n");
}

module.exports = { packageRoot, markdownScript };
