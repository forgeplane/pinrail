// Builds the pinrail CLI for a release bundle and puts it where Tauri looks
// for a sidecar: src-tauri/binaries/<name>-<target triple>.
//
// On Linux the name is `pinrail`: the .deb and .rpm install it as
// /usr/bin/pinrail, beside the app renamed pinrail-desktop
// (tauri.release.linux.conf.json). On macOS it is `pinrail-cli`, since the
// app's own binary is `Pinrail` and a case-insensitive volume would not tell
// the two apart; Settings links it into ~/.local/bin as `pinrail`.
//
//   node scripts/build-cli.mjs                          # the host's triple
//   node scripts/build-cli.mjs universal-apple-darwin   # arm64, x86_64, and the two joined with lipo
//   node scripts/build-cli.mjs x86_64-unknown-linux-gnu
//
// The release build merges src-tauri/tauri.release.conf.json (and, on Linux,
// tauri.release.linux.conf.json), which name the sidecar; a development
// build does not, so it never needs this.
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
  return path.join(cli, "target", target, "release", "pinrail");
}

const target = process.argv[2] ?? host();
const name = target.includes("-linux-") ? "pinrail" : "pinrail-cli";
fs.mkdirSync(out, { recursive: true });

/** Puts a built binary where Tauri looks for the sidecar of `triple`. */
function place(from, triple) {
  const to = path.join(out, `${name}-${triple}`);
  fs.copyFileSync(from, to);
  fs.chmodSync(to, 0o755);
  return to;
}

const written = [];
if (target === "universal-apple-darwin") {
  // A universal build builds each architecture in turn and asks for that
  // architecture's sidecar, so both are placed as well as the merged one.
  const arches = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
  const parts = arches.map((triple) => place(build(triple), triple));
  const merged = path.join(out, `${name}-${target}`);
  run("lipo", ["-create", "-output", merged, ...parts]);
  fs.chmodSync(merged, 0o755);
  written.push(...parts, merged);
} else {
  written.push(place(build(target), target));
}
console.log(`cli: ${written.map((f) => path.relative(app, f)).join(", ")}`);
