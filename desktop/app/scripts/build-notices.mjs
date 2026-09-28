// Writes notices/THIRD_PARTY_NOTICES.txt: the licence of everything Pinrail's
// app ships that someone else wrote. The Rust crates of the desktop app and of
// the bundled CLI come from cargo-about; the npm packages in the UI bundle from
// the production build (vite.config.ts writes notices/npm.json); the Lucide
// icons in the official plugins and the font the window draws in are added
// by hand. Each distinct licence
// text is printed once, after the packages that use it.
//
//   npm run build && npm run notices
//
// Needs cargo-about on the PATH. The release build runs it before bundling.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const app = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const root = path.resolve(app, "..", "..");
const out = path.join(app, "notices");
const npmFile = path.join(out, "npm.json");

/**
 * about.toml with deny.toml's allowed licences as its accepted list, written
 * to a temporary file: one list, which CI's licence check already enforces.
 */
function aboutConfig() {
  const deny = fs.readFileSync(path.join(root, "deny.toml"), "utf8");
  const allow = deny.match(/^\[licenses\][^[]*?^allow = \[([^\]]*)\]/m);
  if (!allow) throw new Error("notices: no allow list under [licenses] in deny.toml");
  const ids = [...allow[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
  const config = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "pinrail-notices-")), "about.toml");
  fs.writeFileSync(config, `${fs.readFileSync(path.join(root, "about.toml"), "utf8")}\naccepted = ${JSON.stringify(ids)}\n`);
  return config;
}

const config = aboutConfig();

/** cargo-about's licences for one workspace's dependencies, as {text, name, packages}. */
function rust(manifest) {
  const json = execFileSync(
    "cargo",
    ["about", "generate", "--format", "json", "--config", config, "--manifest-path", manifest, "--fail"],
    { encoding: "utf8", maxBuffer: 256 * 1024 * 1024, stdio: ["ignore", "pipe", "inherit"] },
  );
  return JSON.parse(json)
    .licenses.map((l) => ({
      name: l.name,
      id: l.id,
      text: l.text,
      // Pinrail's own crates live in this repository and are not third-party
      packages: l.used_by.filter((u) => !u.crate.manifest_path.startsWith(root + path.sep)).map((u) => `${u.crate.name} ${u.crate.version}`),
    }))
    .filter((l) => l.packages.length > 0);
}

/**
 * A package the app ships whole rather than through the UI bundle — a font,
 * or a file it serves to plugin views — from its own LICENSE. The bundle's
 * licence plugin never sees these, so they are named here.
 */
function shipped(name, licence) {
  const dir = path.join(app, "node_modules", name);
  const version = JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8")).version;
  return {
    ...licence,
    text: fs.readFileSync(path.join(dir, "LICENSE"), "utf8"),
    packages: [`${name} ${version} (npm)`],
  };
}

if (!fs.existsSync(npmFile)) {
  console.error("notices: no notices/npm.json; run the production build (npm run build) first");
  process.exit(1);
}

const entries = [
  ...rust(path.join(root, "desktop", "Cargo.toml")),
  ...rust(path.join(root, "cli", "Cargo.toml")),
  ...JSON.parse(fs.readFileSync(npmFile, "utf8")).map((d) => ({
    name: d.license,
    id: d.license,
    text: [d.text, d.notice].filter(Boolean).join("\n\n") || `${d.license} (no licence text shipped with the package)`,
    packages: [`${d.name} ${d.version} (npm)`],
  })),
  {
    name: "ISC License",
    id: "ISC",
    text: fs.readFileSync(path.join(root, "pinrail-plugin", "licenses", "lucide-icons.txt"), "utf8"),
    packages: ["Lucide icons, in the official plugins"],
  },
  // the font files the window draws in, imported as CSS
  shipped("@fontsource-variable/inter", { name: "SIL Open Font License 1.1", id: "OFL-1.1" }),
  // the markdown parser the app serves inside /sdk/v1/pinrail-plugin.js, which
  // every plugin view renders markdown with
  shipped("markdown-it", { name: "MIT License", id: "MIT" }),
];

// one section per distinct text, the packages sorted and without repeats
const byText = new Map();
for (const e of entries) {
  const key = e.text.trim();
  const section = byText.get(key) ?? { id: e.id, name: e.name, text: key, packages: new Set() };
  for (const p of e.packages) section.packages.add(p);
  byText.set(key, section);
}
const sections = [...byText.values()].sort((a, b) => a.id.localeCompare(b.id) || [...a.packages][0].localeCompare([...b.packages][0]));

const rule = "=".repeat(78);
const lines = [
  "Third-party notices for Pinrail",
  "",
  "Pinrail is licensed under the Apache License 2.0; see LICENSE and NOTICE.",
  "It includes the following software, each under the licence shown after the",
  "packages that use it.",
  "",
];
for (const s of sections) {
  lines.push(rule, "", [...s.packages].sort().join("\n"), "", `${s.name} (${s.id})`, "", s.text, "");
}
fs.mkdirSync(out, { recursive: true });
const target = path.join(out, "THIRD_PARTY_NOTICES.txt");
fs.writeFileSync(target, lines.join("\n"));
const packages = sections.reduce((n, s) => n + s.packages.size, 0);
console.log(`notices: ${packages} packages under ${sections.length} licence texts in ${path.relative(app, target)}`);
