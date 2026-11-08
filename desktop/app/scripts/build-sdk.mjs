// Assembles the directory the server serves at /sdk/v1/: the plugin SDK from
// sdk/src, the optional Markdown and highlighting scripts, and the font the window
// itself is drawn in. Plugins bring their own icons. The font comes from the same
// package the shell bundles, so a plugin panel and the window around it are
// set in one typeface. The app carries the files so it draws the same with no
// network at all; the SDK dev server points the same import at a CDN instead
// of shipping fonts in the npm package.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { highlightScript, markdownScript } from "../../../sdk/lib/paths.cjs";

const app = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const sdkSrc = path.resolve(app, "..", "..", "sdk", "src");
const font = path.resolve(app, "node_modules", "@fontsource-variable", "inter");
const out = path.join(app, "sdk", "v1");

// Every file of the directory, by its path in it. They are written below.
const files = new Map();
// markdown.js and highlight.js are built below rather than copied: they
// carry a parser and a highlighter
const built = ["markdown.js", "highlight.js"];
for (const file of fs.readdirSync(sdkSrc)) {
  if (built.includes(file)) continue;
  files.set(file, fs.readFileSync(path.join(sdkSrc, file)));
}
// the weight axis, every script, normal only: what the shell imports
const faces = fs.readdirSync(path.join(font, "files")).filter((f) => f.endsWith("-wght-normal.woff2"));
for (const file of faces) {
  files.set(path.join("files", file), fs.readFileSync(path.join(font, "files", file)));
}
// the SDK stylesheet imports ./fonts.css; wght.css is that file, and it
// points at ./files/, which is where the faces now sit
files.set("fonts.css", fs.readFileSync(path.join(font, "wght.css")));

// The Markdown renderer with its parser. The parser comes from the app's own
// dependencies: a release installs no others.
files.set("markdown.js", Buffer.from(markdownScript(app, sdkSrc)));
// The highlighter with its languages, from the app's dependencies too.
files.set("highlight.js", Buffer.from(highlightScript(app, sdkSrc)));

// Only a file whose content changed is written, and a file no longer built
// is removed: the desktop crate's build script watches this directory, and
// a file written again, unchanged, would make cargo rebuild the app.
for (const [name, bytes] of files) {
  const target = path.join(out, name);
  if (fs.existsSync(target) && fs.readFileSync(target).equals(bytes)) continue;
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, bytes);
}
if (fs.existsSync(out)) {
  for (const name of fs.readdirSync(out, { recursive: true })) {
    const target = path.join(out, name);
    if (!files.has(name) && fs.statSync(target).isFile()) fs.rmSync(target);
  }
}

console.log(
  `sdk: ${fs.readdirSync(sdkSrc).length} SDK files and ${faces.length} font files in ${path.relative(app, out)}`,
);
