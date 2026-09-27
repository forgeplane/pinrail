// The docs' generated reference pages: the CLI's from its own definition,
// the settings and the manifest from the core's tables.
//
//   node scripts/docs-generate.mjs
//
// Run by `mise run docs:generate` and before every website build.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const out = path.join(root, "docs", "reference");

const pages = {
  "cli.md": ["--manifest-path", "cli/Cargo.toml", "--features", "docs", "--", "--markdown-help"],
  "settings.md": ["--manifest-path", "desktop/Cargo.toml", "-p", "pinrail-core", "--features", "docs", "--bin", "pinrail-docs", "--", "settings"],
  "manifest.md": ["--manifest-path", "desktop/Cargo.toml", "-p", "pinrail-core", "--features", "docs", "--bin", "pinrail-docs", "--", "manifest"],
};

fs.mkdirSync(out, { recursive: true });
for (const [page, args] of Object.entries(pages)) {
  const text = execFileSync("cargo", ["run", "-q", ...args], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
  fs.writeFileSync(path.join(out, page), text);
}
