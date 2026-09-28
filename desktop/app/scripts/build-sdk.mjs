// Assembles the directory the server serves at /sdk/v1/: the plugin SDK from
// pinrail-plugin/src, the optional markdown module, and the font the window
// itself is drawn in. Plugins bring their own icons. The font comes from the same
// package the shell bundles, so a plugin panel and the window around it are
// set in one typeface. The app carries the files so it draws the same with no
// network at all; the SDK dev server points the same import at a CDN instead
// of shipping fonts in the npm package.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { sdkScript } from "../../../pinrail-plugin/lib/paths.cjs";

const app = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const sdkSrc = path.resolve(app, "..", "..", "pinrail-plugin", "src");
const font = path.resolve(app, "node_modules", "@fontsource-variable", "inter");
const out = path.join(app, "sdk", "v1");

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });
// pinrail-plugin.js is assembled below rather than copied: it carries a parser
for (const file of fs.readdirSync(sdkSrc)) {
  if (file === "pinrail-plugin.js") continue;
  fs.copyFileSync(path.join(sdkSrc, file), path.join(out, file));
}
// the weight axis, every script, normal only: what the shell imports
fs.mkdirSync(path.join(out, "files"), { recursive: true });
const faces = fs.readdirSync(path.join(font, "files")).filter((f) => f.endsWith("-wght-normal.woff2"));
for (const file of faces) {
  fs.copyFileSync(path.join(font, "files", file), path.join(out, "files", file));
}
// the SDK stylesheet imports ./fonts.css; wght.css is that file, and it
// points at ./files/, which is where the faces now sit
fs.copyFileSync(path.join(font, "wght.css"), path.join(out, "fonts.css"));

// The SDK and the markdown parser it renders with, as the one script a view
// loads. The parser comes from the app's own dependencies: a release installs
// no others.
fs.writeFileSync(path.join(out, "pinrail-plugin.js"), sdkScript(app, sdkSrc));

console.log(
  `sdk: ${fs.readdirSync(sdkSrc).length} SDK files and ${faces.length} font files in ${path.relative(app, out)}`,
);
