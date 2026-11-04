// The plugins the website's gallery lists (website/src/pages/plugins/):
// the official ones in plugins/, and the samples in a checkout of
// forgeplane/pinrail-plugins beside this repository, or the folder that
// PINRAIL_PLUGINS_DIR names. The site builds without that checkout, so this
// writes what it needs into the website: each plugin's manifest fields in
// website/src/data/plugins.json, and, for a sample with no screenshot of its
// own on the site, the README's screenshot in website/src/assets/plugins/.
//
//   node scripts/plugin-gallery.mjs
//
// Run it again when a plugin's manifest or screenshot changes.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const samples = path.join(process.env.PINRAIL_PLUGINS_DIR ?? path.join(root, "..", "pinrail-plugins"), "plugins");
const shots = path.join(root, "website", "src", "assets", "screenshots");
const readmeShots = path.join(root, "website", "src", "assets", "plugins");
const out = path.join(root, "website", "src", "data", "plugins.json");

// in the order the gallery shows them
const OFFICIAL = ["list", "feedback", "code-review", "change-map", "markdown", "image"];
const SAMPLES = [
  "db-schema",
  "canvas",
  "artifact",
  "email",
  "audio",
  "video",
  "visual-diff",
  "model-3d",
  "logo",
  "palette",
  "motion",
  "calendar",
  "trade",
];
// the setup installs these (desktop/core/src/plugins/catalog.rs)
const RECOMMENDED = new Set(["list", "feedback"]);
// the screenshot run's view shots, where a plugin's name is not the shot's
const SHOT = { "code-review": "review-view", "model-3d": "model-view" };

const read = (file) => JSON.parse(fs.readFileSync(file, "utf8"));

function entry(dir, name, official) {
  const m = read(path.join(dir, "manifest.json"));
  const shot = SHOT[name] ?? `${name}-view`;
  const hasShot = fs.existsSync(path.join(shots, `${shot}-light.png`));
  let image;
  if (hasShot) {
    image = { shot, dark: fs.existsSync(path.join(shots, `${shot}-dark.png`)) };
  } else {
    const readme = path.join(dir, "screenshot.png");
    if (!fs.existsSync(readme)) throw new Error(`${name} has no screenshot on the site and no screenshot.png`);
    fs.mkdirSync(readmeShots, { recursive: true });
    fs.copyFileSync(readme, path.join(readmeShots, `${name}.png`));
    image = { readme: name };
  }
  const pkg = path.join(dir, "package.json");
  return {
    name,
    title: m.title,
    version: m.version,
    description: m.description,
    use_when: m.use_when,
    official,
    recommended: RECOMMENDED.has(name),
    takes_files: Boolean(m.attachments),
    // a sample that bundles libraries into its view is built before it installs
    build: !official && fs.existsSync(pkg) && Boolean(read(pkg).scripts?.build),
    docs: official
      ? `/docs/plugins/${name}/`
      : `https://github.com/forgeplane/pinrail-plugins/tree/main/plugins/${name}`,
    image,
  };
}

if (!fs.existsSync(samples)) throw new Error(`no sample plugins in ${samples}: check out forgeplane/pinrail-plugins`);
const plugins = [
  ...OFFICIAL.map((n) => entry(path.join(root, "plugins", n), n, true)),
  ...SAMPLES.map((n) => entry(path.join(samples, n), n, false)),
];
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, JSON.stringify(plugins, null, 2) + "\n");
console.log(`plugin-gallery: ${plugins.length} plugins in ${path.relative(root, out)}`);
