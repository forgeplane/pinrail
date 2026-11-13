// Builds pinrail-host, the terminal host, for a release bundle and puts it
// where Tauri looks for a sidecar: src-tauri/binaries/pinrail-host-<target
// triple>. The core copies it into the data folder and starts it from
// there, so that it outlives the app. It runs on macOS and Linux only.
//
//   node scripts/build-host.mjs                          # the host's triple
//   node scripts/build-host.mjs universal-apple-darwin   # arm64, x86_64, and the two joined with lipo
//   node scripts/build-host.mjs x86_64-unknown-linux-gnu
//
// The release build merges src-tauri/tauri.release.conf.json (and, on Linux,
// tauri.release.linux.conf.json), which name the sidecar.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const app = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const workspace = path.resolve(app, "..");
const out = path.join(app, "src-tauri", "binaries");

const run = (cmd, args) => execFileSync(cmd, args, { stdio: "inherit" });
const host = () => /host: (\S+)/.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))[1];

/** Builds for one target and returns the binary's path. */
function build(target) {
  run("cargo", [
    "build",
    "--release",
    "--locked",
    "--manifest-path",
    path.join(workspace, "Cargo.toml"),
    "--package",
    "pinrail-host",
    "--target",
    target,
  ]);
  return path.join(workspace, "target", target, "release", "pinrail-host");
}

const target = process.argv[2] ?? host();
if (target.includes("-windows-")) {
  console.error("pinrail-host runs only on macOS and Linux");
  process.exit(1);
}
fs.mkdirSync(out, { recursive: true });

/** Puts a built binary where Tauri looks for the sidecar of `triple`. */
function place(from, triple) {
  const to = path.join(out, `pinrail-host-${triple}`);
  fs.copyFileSync(from, to);
  fs.chmodSync(to, 0o755);
  return to;
}

const written = [];
if (target === "universal-apple-darwin") {
  const arches = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
  const parts = arches.map((triple) => place(build(triple), triple));
  const merged = path.join(out, `pinrail-host-${target}`);
  run("lipo", ["-create", "-output", merged, ...parts]);
  fs.chmodSync(merged, 0o755);
  written.push(...parts, merged);
} else {
  written.push(place(build(target), target));
}
console.log(`host: ${written.map((f) => path.relative(app, f)).join(", ")}`);
