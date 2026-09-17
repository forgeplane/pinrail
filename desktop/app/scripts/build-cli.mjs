// Builds the wicket CLI for a release bundle and puts it where Tauri looks
// for a sidecar: src-tauri/binaries/wicket-cli-<target triple>. The bundle
// ships it as `wicket-cli` beside the app's own binary, whose name, `Wicket`,
// a case-insensitive macOS volume would not tell apart from `wicket`.
//
//   node scripts/build-cli.mjs                          # the host's triple
//   node scripts/build-cli.mjs universal-apple-darwin   # arm64 and x86_64, joined with lipo
//   node scripts/build-cli.mjs x86_64-unknown-linux-gnu
//
// The release build merges src-tauri/tauri.release.conf.json, which names
// the sidecar; a development build does not, so it never needs this.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const app = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const cli = path.resolve(app, "..", "..", "cli");
const out = path.join(app, "src-tauri", "binaries");

const run = (cmd, args) => execFileSync(cmd, args, { stdio: "inherit" });
const host = () => /host: (\S+)/.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))[1];

/** Builds for one target and returns the binary's path. */
function build(target) {
  run("cargo", ["build", "--release", "--locked", "--manifest-path", path.join(cli, "Cargo.toml"), "--target", target]);
  return path.join(cli, "target", target, "release", "wicket");
}

const target = process.argv[2] ?? host();
fs.mkdirSync(out, { recursive: true });
const dest = path.join(out, `wicket-cli-${target}`);

if (target === "universal-apple-darwin") {
  const parts = ["aarch64-apple-darwin", "x86_64-apple-darwin"].map(build);
  run("lipo", ["-create", "-output", dest, ...parts]);
} else {
  fs.copyFileSync(build(target), dest);
}
fs.chmodSync(dest, 0o755);
console.log(`cli: ${path.relative(app, dest)}`);
