// Builds pinrail-host, the terminal host, for a release bundle. The core
// copies it into the data folder and starts it from there, so that it
// outlives the app. It runs on macOS and Linux only.
//
// On macOS the host is an app bundle, src-tauri/binaries/Pinrail Host.app,
// which tauri.release.conf.json places in Pinrail.app's Contents/Helpers.
// macOS names the bundle in the privacy prompts that agents cause, and keeps
// the person's answers by its bundle identifier. The bundle is signed with
// HOST_SIGNING_IDENTITY, the hardened runtime and the host's entitlements,
// or ad hoc when HOST_SIGNING_IDENTITY is not set. A --dev build without
// HOST_SIGNING_IDENTITY is signed with the keychain's Developer ID
// Application identity when there is one, so that macOS keeps the grants
// given to the dev host across rebuilds; HOST_SIGNING_IDENTITY=- signs it
// ad hoc. On Linux the host is the sidecar
// src-tauri/binaries/pinrail-host-<target triple>.
//
//   node scripts/build-host.mjs                          # the host's triple
//   node scripts/build-host.mjs universal-apple-darwin   # arm64, x86_64, and the two joined with lipo
//   node scripts/build-host.mjs x86_64-unknown-linux-gnu
//   node scripts/build-host.mjs --dev                    # a debug build, as target/debug/Pinrail Host.app on macOS
//
// The release build merges src-tauri/tauri.release.conf.json (and, on Linux,
// tauri.release.linux.conf.json), which name the bundle or the sidecar.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const app = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const workspace = path.resolve(app, "..");
const out = path.join(app, "src-tauri", "binaries");
const macos = path.join(workspace, "host", "macos");

const run = (cmd, args) => execFileSync(cmd, args, { stdio: "inherit" });
const host = () => /host: (\S+)/.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))[1];

/** Builds for one target, or for the host's in debug, and returns the binary's path. */
function build(target, { dev = false } = {}) {
  const args = [
    "build",
    "--locked",
    "--manifest-path",
    path.join(workspace, "Cargo.toml"),
    "--package",
    "pinrail-host",
  ];
  if (dev) {
    run("cargo", ["-q", ...args]);
    return path.join(workspace, "target", "debug", "pinrail-host");
  }
  run("cargo", [...args, "--release", "--target", target]);
  return path.join(workspace, "target", target, "release", "pinrail-host");
}

/** The version of the desktop workspace, which the host reports. */
function version() {
  const manifest = fs.readFileSync(path.join(workspace, "Cargo.toml"), "utf8");
  return /^version = "([^"]+)"/m.exec(manifest)[1];
}

/** The first Developer ID Application identity in the keychain, if any. */
function developerId() {
  try {
    const found = execFileSync("security", ["find-identity", "-v", "-p", "codesigning"], { encoding: "utf8" });
    return /"(Developer ID Application: [^"]+)"/.exec(found)?.[1] ?? null;
  } catch {
    return null;
  }
}

/**
 * Makes `Pinrail Host.app` in `folder` from the host binary `binary`, and
 * signs it with `identity`, or ad hoc with "-". Returns the bundle's path.
 */
function bundle(binary, folder, identity) {
  const bundlePath = path.join(folder, "Pinrail Host.app");
  fs.rmSync(bundlePath, { recursive: true, force: true });
  fs.mkdirSync(path.join(bundlePath, "Contents", "MacOS"), { recursive: true });
  const info = fs.readFileSync(path.join(macos, "Info.plist"), "utf8").replaceAll("@VERSION@", version());
  fs.writeFileSync(path.join(bundlePath, "Contents", "Info.plist"), info);
  const executable = path.join(bundlePath, "Contents", "MacOS", "pinrail-host");
  fs.copyFileSync(binary, executable);
  fs.chmodSync(executable, 0o755);
  // a secure timestamp, which notarization needs, only for a release
  const timestamp = identity === "-" || dev ? [] : ["--timestamp"];
  run("codesign", [
    "--force",
    "--sign",
    identity,
    ...timestamp,
    "--options",
    "runtime",
    "--entitlements",
    path.join(macos, "Entitlements.plist"),
    "--identifier",
    "dev.pinrail.host",
    bundlePath,
  ]);
  run("codesign", ["--verify", "--strict", bundlePath]);
  return bundlePath;
}

const dev = process.argv.includes("--dev");
const target = process.argv.slice(2).find((a) => !a.startsWith("--")) ?? host();
if (target.includes("-windows-")) {
  console.error("pinrail-host runs only on macOS and Linux");
  process.exit(1);
}
const apple = target.endsWith("-apple-darwin");

const identity = process.env.HOST_SIGNING_IDENTITY || (dev && apple && developerId()) || "-";
const written = [];
if (dev) {
  // tauri dev's app finds the bundle beside its own binary
  const binary = build(target, { dev });
  written.push(apple ? bundle(binary, path.dirname(binary), identity) : binary);
} else {
  fs.mkdirSync(out, { recursive: true });

  /** Puts a built binary where Tauri looks for the sidecar of `triple`. */
  const place = (from, triple) => {
    const to = path.join(out, `pinrail-host-${triple}`);
    fs.copyFileSync(from, to);
    fs.chmodSync(to, 0o755);
    return to;
  };

  let binary;
  if (target === "universal-apple-darwin") {
    const arches = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
    const parts = arches.map((triple) => place(build(triple), triple));
    binary = path.join(out, `pinrail-host-${target}`);
    run("lipo", ["-create", "-output", binary, ...parts]);
    fs.chmodSync(binary, 0o755);
    written.push(...parts, binary);
  } else {
    binary = place(build(target), target);
    written.push(binary);
  }
  if (apple) written.push(bundle(binary, out, identity));
}
console.log(`host: ${written.map((f) => path.relative(app, f)).join(", ")}`);
if (apple) console.log(`host: signed ${identity === "-" ? "ad hoc" : `with ${identity}`}`);
