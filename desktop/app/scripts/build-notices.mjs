// Writes notices/THIRD_PARTY_NOTICES.txt: the licence of everything Wicket's
// app ships that someone else wrote. The Rust crates of the desktop app and of
// the bundled CLI come from cargo-about; the npm packages in the UI bundle from
// the production build (vite.config.ts writes notices/npm.json); the Lucide
// icons the app serves to plugins and the font the window draws in are added
// by hand. Each distinct licence
// text is printed once, after the packages that use it.
//
//   npm run build && npm run notices
//
// Needs cargo-about on the PATH. The release build runs it before bundling.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const app = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const root = path.resolve(app, "..", "..");
const out = path.join(app, "notices");
const npmFile = path.join(out, "npm.json");

/** cargo-about's licences for one workspace's dependencies, as {text, name, packages}. */
function rust(manifest) {
  const json = execFileSync(
    "cargo",
    ["about", "generate", "--format", "json", "--config", path.join(root, "about.toml"), "--manifest-path", manifest, "--fail"],
    { encoding: "utf8", maxBuffer: 256 * 1024 * 1024, stdio: ["ignore", "pipe", "inherit"] },
  );
  return JSON.parse(json)
    .licenses.map((l) => ({
      name: l.name,
      id: l.id,
      text: l.text,
      // Wicket's own crates live in this repository and are not third-party
      packages: l.used_by.filter((u) => !u.crate.manifest_path.startsWith(root + path.sep)).map((u) => `${u.crate.name} ${u.crate.version}`),
    }))
    .filter((l) => l.packages.length > 0);
}

/** A font package whose files ship in the bundle, from its own LICENSE. */
function font(name) {
  const dir = path.join(app, "node_modules", name);
  const version = JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8")).version;
  return {
    name: "SIL Open Font License 1.1",
    id: "OFL-1.1",
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
    text: fs.readFileSync(path.join(root, "wicket-plugin", "licenses", "lucide-icons.txt"), "utf8"),
    packages: ["Lucide icons (lucide-static 1.45.0)"],
  },
  // The font files the window draws in. They are imported as CSS, so the
  // bundle's licence plugin never sees the package; its licence is read from
  // the copy npm installed.
  font("@fontsource-variable/inter"),
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
  "Third-party notices for Wicket",
  "",
  "Wicket is licensed under the Apache License 2.0; see LICENSE and NOTICE.",
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
