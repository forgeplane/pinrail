//! Builds libghostty-vt from the release `ghostty.lock` pins, and links it
//! statically.
//!
//! The two tarballs are downloaded once, checked against their SHA-256
//! sums, and kept with the built library in a cache shared by every build
//! profile, under the target folder's `ghostty/<version>`. The build itself
//! runs offline: Zig takes its packages from the dependency tarball.
//!
//! It builds nothing on Windows, where the terminal host does not run.
//!
//! - `PINRAIL_GHOSTTY_STUB=1` builds nothing and links nothing, for a type
//!   check or clippy on a machine without Zig.
//! - `PINRAIL_GHOSTTY_SRC=<folder>` takes the tarballs from a folder, such
//!   as the one `mise run ghostty:fetch` fills, instead of downloading them.
//! - `PINRAIL_GHOSTTY_CACHE=<folder>` keeps the cache there instead.
//! - `ZIG=<path>` names the Zig to build with; otherwise `zig` on the PATH.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// What ghostty.lock pins.
struct Lock {
    version: String,
    zig: String,
    source: (String, String),
    deps: (String, String),
}

/// Reads ghostty.lock: `key = "value"` lines, under `[source]` and `[deps]`
/// for the two tarballs.
fn lock(text: &str) -> Lock {
    let mut section = "";
    let mut values = std::collections::HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name;
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = match section {
                "" => key.trim().to_string(),
                _ => format!("{section}.{}", key.trim()),
            };
            values.insert(key, value.trim().trim_matches('"').to_string());
        }
    }
    let get = |key: &str| {
        values
            .get(key)
            .cloned()
            .unwrap_or_else(|| panic!("ghostty.lock has no {key}"))
    };
    Lock {
        version: get("version"),
        zig: get("zig"),
        source: (get("source.url"), get("source.sha256")),
        deps: (get("deps.url"), get("deps.sha256")),
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // set once the library is built and linked, for the tests that need it
    println!("cargo::rustc-check-cfg=cfg(ghostty_built)");
    println!("cargo:rerun-if-changed=ghostty.lock");
    for var in [
        "PINRAIL_GHOSTTY_STUB",
        "PINRAIL_GHOSTTY_SRC",
        "PINRAIL_GHOSTTY_CACHE",
        "ZIG",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    // the terminal host runs agents in PTYs, so only on Unix
    if env::var_os("CARGO_FEATURE_GHOSTTY").is_none() || env::var_os("CARGO_CFG_UNIX").is_none() {
        return;
    }
    if env::var("PINRAIL_GHOSTTY_STUB").is_ok_and(|v| v == "1") {
        return;
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let lock = lock(&fs::read_to_string(manifest.join("ghostty.lock")).unwrap());
    let target = env::var("TARGET").unwrap();
    let cache = cache_dir().join(&lock.version);
    fs::create_dir_all(&cache).unwrap();

    let prefix = cache.join(&target);
    if !prefix.join("lib").join("libghostty-vt.a").exists() {
        let zig = zig(&lock);
        let source = unpacked(&cache, &lock.source, &lock.version);
        let deps = unpacked(&cache, &lock.deps, &lock.version);
        build(&zig, &source, &deps, &prefix, &target);
    }
    // a folder with the static archive alone: beside the shared library,
    // the macOS linker takes the shared one
    let archive = prefix.join("static");
    if !archive.join("libghostty-vt.a").exists() {
        fs::create_dir_all(&archive).unwrap();
        fs::copy(
            prefix.join("lib").join("libghostty-vt.a"),
            archive.join("libghostty-vt.a"),
        )
        .unwrap();
    }
    // for the test that checks every part of it is in the notices
    println!(
        "cargo:rustc-env=PINRAIL_GHOSTTY_ARCHIVE={}",
        archive.join("libghostty-vt.a").display()
    );
    println!("cargo:rustc-link-search=native={}", archive.display());
    println!("cargo:rustc-link-lib=static=ghostty-vt");
    println!("cargo:rustc-cfg=ghostty_built");
}

/// Where the tarballs and the built library are kept: `PINRAIL_GHOSTTY_CACHE`,
/// or `ghostty` in the target folder, which every build profile shares.
fn cache_dir() -> PathBuf {
    if let Some(dir) = env::var_os("PINRAIL_GHOSTTY_CACHE") {
        return PathBuf::from(dir);
    }
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    // cargo marks the target folder with CACHEDIR.TAG, and the folder of
    // each --target inside it too: the outermost is the one all share
    out.ancestors()
        .filter(|dir| dir.join("CACHEDIR.TAG").exists())
        .last()
        .unwrap_or(&out)
        .join("ghostty")
}

/// The folder a tarball unpacks to, downloading, checking and unpacking it
/// first if it is not there yet.
fn unpacked(cache: &Path, (url, sha256): &(String, String), version: &str) -> PathBuf {
    let file = url.rsplit('/').next().unwrap();
    let folder = cache.join(file.strip_suffix(".tar.gz").unwrap());
    // written once the folder is complete: a cache that keeps folders but
    // drops their files, as CI's Rust cache does, leaves a folder without it
    let unpacked = folder.join(".pinrail-unpacked");
    if unpacked.exists() {
        return folder;
    }
    let tarball = match env::var_os("PINRAIL_GHOSTTY_SRC") {
        Some(dir) => PathBuf::from(dir).join(file),
        None => {
            let tarball = cache.join(file);
            if !tarball.exists() {
                download(url, &tarball);
            }
            tarball
        }
    };
    let actual = sha256_of(&tarball);
    if &actual != sha256 {
        panic!(
            "{} has SHA-256 {actual}, but ghostty.lock expects {sha256} for libghostty-vt {version}",
            tarball.display()
        );
    }
    // unpacked beside the cache and renamed into place, so that a build
    // that stops halfway leaves nothing a later one would take as complete
    let partial = cache.join(format!("{file}.unpacking"));
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir_all(&partial).unwrap();
    run(Command::new("tar")
        .arg("-xzf")
        .arg(&tarball)
        .arg("-C")
        .arg(&partial));
    let inner = partial.join(folder.file_name().unwrap());
    let _ = fs::remove_dir_all(&folder);
    if fs::rename(&inner, &folder).is_err() && !unpacked.exists() {
        panic!("could not move {} into place", inner.display());
    }
    fs::write(&unpacked, "").unwrap();
    let _ = fs::remove_dir_all(&partial);
    folder
}

fn download(url: &str, to: &Path) {
    let partial = to.with_extension("download");
    run(Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--output",
        ])
        .arg(&partial)
        .arg(url));
    fs::rename(&partial, to).unwrap();
}

fn sha256_of(file: &Path) -> String {
    let mut hasher = Sha256::new();
    let mut reader =
        fs::File::open(file).unwrap_or_else(|e| panic!("reading {}: {e}", file.display()));
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    format!("{:x}", hasher.finalize())
}

/// Zig's name for a Rust target, or `None` for the machine's own.
fn zig_target(target: &str) -> Option<&'static str> {
    if env::var("HOST").ok().as_deref() == Some(target) {
        return None;
    }
    Some(match target {
        "aarch64-apple-darwin" => "aarch64-macos",
        "x86_64-apple-darwin" => "x86_64-macos",
        "x86_64-unknown-linux-gnu" => "x86_64-linux-gnu",
        "aarch64-unknown-linux-gnu" => "aarch64-linux-gnu",
        "x86_64-unknown-linux-musl" => "x86_64-linux-musl",
        "aarch64-unknown-linux-musl" => "aarch64-linux-musl",
        other => panic!("libghostty-vt: no Zig target is known for {other}"),
    })
}

/// The Zig to build with, checked to be the version ghostty.lock names.
fn zig(lock: &Lock) -> String {
    let zig = env::var("ZIG").unwrap_or_else(|_| "zig".to_string());
    let version = Command::new(&zig)
        .arg("version")
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "libghostty-vt needs Zig {}, and `{zig}` did not run ({e}). Install it with \
                 `mise install`, or set PINRAIL_GHOSTTY_STUB=1 to type-check without it.",
                lock.zig
            )
        });
    let version = String::from_utf8_lossy(&version.stdout).trim().to_string();
    if !version.starts_with(&format!("{}.", lock.zig)) {
        panic!(
            "libghostty-vt needs Zig {}, but `{zig}` is {version}. Install it with `mise install`.",
            lock.zig
        );
    }

    zig
}

/// Builds the static library into `prefix`, offline, with an empty Zig
/// cache of its own: Zig 0.17 can crash on a cache that already holds an
/// archive of a package.
fn build(zig: &str, source: &Path, deps: &Path, prefix: &Path, target: &str) {
    let work = prefix.with_extension("building");
    let _ = fs::remove_dir_all(&work);
    let zig_cache = work.join("zig-cache");
    let installed = work.join("prefix");
    fs::create_dir_all(&zig_cache).unwrap();
    let mut command = Command::new(zig);
    command
        .current_dir(source)
        .env("ZIG_GLOBAL_CACHE_DIR", &zig_cache)
        .env_remove("ZIG_LOCAL_CACHE_DIR")
        .arg("build")
        .arg("--prefix")
        .arg(&installed)
        .arg("--system")
        .arg(deps)
        .args([
            "-Doptimize=ReleaseFast",
            "-Dcpu=baseline",
            "-Demit-xcframework=false",
            "-Dstrip=true",
        ]);
    if let Some(zig_target) = zig_target(target) {
        command.arg(format!("-Dtarget={zig_target}"));
    }
    run(&mut command);
    // a folder left without the library, as a cache can leave it
    let _ = fs::remove_dir_all(prefix);
    if fs::rename(&installed, prefix).is_err()
        && !prefix.join("lib").join("libghostty-vt.a").exists()
    {
        panic!("could not move {} into place", installed.display());
    }
    let _ = fs::remove_dir_all(&work);
}

fn run(command: &mut Command) {
    let status = command
        .status()
        .unwrap_or_else(|e| panic!("running {command:?}: {e}"));
    if !status.success() {
        panic!("{command:?} failed with {status}");
    }
}
