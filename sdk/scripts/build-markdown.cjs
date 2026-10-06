// Writes dist/markdown.js, the file the package exports as
// ./sdk/v1/markdown.js. Run before the package is packed.
const fs = require("node:fs");
const path = require("node:path");
const { markdownScript } = require("../lib/paths.cjs");

const root = path.dirname(__dirname);
fs.mkdirSync(path.join(root, "dist"), { recursive: true });
fs.writeFileSync(path.join(root, "dist", "markdown.js"), markdownScript(root));
