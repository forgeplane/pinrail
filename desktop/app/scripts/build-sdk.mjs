// Assembles the directory the server serves at /sdk/v1/: the plugin SDK from
// wicket-plugin/src and the icon set plugin views draw from. The icons come from
// the pinned lucide-static package, the same release wicket-plugin depends on.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const app = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const sdkSrc = path.resolve(app, "..", "..", "wicket-plugin", "src");
const icons = path.resolve(app, "node_modules", "lucide-static", "icons");
const out = path.join(app, "sdk", "v1");

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(path.join(out, "icons"), { recursive: true });
for (const file of fs.readdirSync(sdkSrc)) {
  fs.copyFileSync(path.join(sdkSrc, file), path.join(out, file));
}
let count = 0;
for (const file of fs.readdirSync(icons)) {
  if (!file.endsWith(".svg")) continue;
  fs.copyFileSync(path.join(icons, file), path.join(out, "icons", file));
  count += 1;
}
console.log(`sdk: ${fs.readdirSync(sdkSrc).length} SDK files and ${count} icons in ${path.relative(app, out)}`);
