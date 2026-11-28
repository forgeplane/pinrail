//! On Windows, puts Microsoft's ConPTY beside the binaries this build
//! makes: `conpty\conpty.dll` and `conpty\<arch>\OpenConsole.exe` for
//! each architecture, x64 and arm64, since conpty.dll starts the console of
//! the machine's own architecture, which an x64 build emulated on ARM64 is
//! not, and falls back to Windows' own without it; in the
//! profile's folder, where `pinrail-host.exe` is, and in its `deps`
//! folder, where the tests run. Windows' own ConPTY drops the escape
//! sequences programs answer with, such as the replies to a device
//! attributes query, so the host loads this one when it is there. The
//! package is downloaded once into the build script's own folder, and it
//! and the two files are checked against their SHA-256 sums.
//!
//! - `PINRAIL_CONPTY_SRC=<folder>` takes the package from a folder instead
//!   of downloading it.
//!
//! The release puts the same files beside the host it bundles.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

const VERSION: &str = "1.24.260710001";
const URL: &str = "https://github.com/microsoft/terminal/releases/download/v1.24.11911.0/Microsoft.Windows.Console.ConPTY.1.24.260710001.nupkg";
const PACKAGE_SHA256: &str = "9382ad7becb7e4d84e300578d8e4f4df28f43d979d9055d978c42913c47e0e9d";

/// For each architecture, ConPTY's name for it and the two files' sums.
const FILES: [(&str, &str, &str, &str); 2] = [
    (
        "x86_64",
        "x64",
        "39fba2713e2495117b1591ae8c32a3b904bea7aa66069cf7815e2844c76d75d8",
        "b7fd936c2668b87b9ecf7b3366dc6568afc1c6f981874cba3e955a1c35cf8160",
    ),
    (
        "aarch64",
        "arm64",
        "db3d173640b172bafd42d5b541b638a9aeec1c7d0e40dd636bf02822a32c912c",
        "ed7622fd0d3bedc9ab9f122f5e58edf0def9e7999224f52dd395ba9f54edbe09",
    ),
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PINRAIL_CONPTY_SRC");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let Some(&(_, name, dll_sha, _)) = FILES.iter().find(|(a, ..)| *a == arch) else {
        panic!("no ConPTY is known for {arch}");
    };
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    // OUT_DIR is <target>/[<triple>/]<profile>/build/<crate>-<hash>/out
    let profile = out.ancestors().nth(3).unwrap().to_path_buf();
    let unpacked = unpacked(&out.join("conpty").join(VERSION));
    let dll = unpacked.join(format!("runtimes/win-{name}/native/conpty.dll"));
    check(&dll, dll_sha);
    let consoles: Vec<(&str, PathBuf)> = FILES
        .iter()
        .map(|&(_, name, _, sha)| {
            let console = unpacked.join(format!("build/native/runtimes/{name}/OpenConsole.exe"));
            check(&console, sha);
            (name, console)
        })
        .collect();
    for dir in [profile.clone(), profile.join("deps")] {
        let conpty = dir.join("conpty");
        copy_if_changed(&dll, &conpty.join("conpty.dll"));
        for (name, console) in &consoles {
            copy_if_changed(console, &conpty.join(name).join("OpenConsole.exe"));
        }
    }
}

/// The unpacked package, downloading, checking and unpacking it first if
/// it is not there yet.
fn unpacked(cache: &Path) -> PathBuf {
    let folder = cache.join("package");
    let done = folder.join(".pinrail-unpacked");
    if done.exists() {
        return folder;
    }
    fs::create_dir_all(cache).unwrap();
    let file = URL.rsplit('/').next().unwrap();
    let package = match env::var_os("PINRAIL_CONPTY_SRC") {
        Some(dir) => PathBuf::from(dir).join(file),
        None => {
            let package = cache.join(file);
            if !package.exists() {
                let partial = package.with_extension("download");
                run(Command::new("curl")
                    .args([
                        "--fail",
                        "--silent",
                        "--show-error",
                        "--location",
                        "--output",
                    ])
                    .arg(&partial)
                    .arg(URL));
                fs::rename(&partial, &package).unwrap();
            }
            package
        }
    };
    check(&package, PACKAGE_SHA256);
    // a NuGet package is a zip file, which Windows' tar unpacks
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    run(Command::new("tar")
        .arg("-xf")
        .arg(&package)
        .arg("-C")
        .arg(&folder));
    fs::write(&done, "").unwrap();
    folder
}

fn check(file: &Path, sha256: &str) {
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
    let actual = format!("{:x}", hasher.finalize());
    if actual != sha256 {
        panic!("{} has SHA-256 {actual}, not {sha256}", file.display());
    }
}

/// Copies `from` over `to` unless `to` already has its content: a running
/// host keeps its OpenConsole.exe open, and a build must not fail on it.
/// The copy is made beside `to` and renamed over it, so that two builds of
/// the crate at once, with and without its server, never see half a file.
fn copy_if_changed(from: &Path, to: &Path) {
    let wanted = fs::read(from).unwrap_or_else(|e| panic!("reading {}: {e}", from.display()));
    if fs::read(to).ok().as_ref() == Some(&wanted) {
        return;
    }
    // a clean build has no folder for it yet
    let folder = to.parent().unwrap();
    fs::create_dir_all(folder).unwrap_or_else(|e| panic!("making {}: {e}", folder.display()));
    let partial = to.with_extension(format!("partial-{}", std::process::id()));
    fs::write(&partial, &wanted).unwrap_or_else(|e| panic!("writing {}: {e}", partial.display()));
    if let Err(error) = fs::rename(&partial, to) {
        let _ = fs::remove_file(&partial);
        // another build put the same file there first
        if fs::read(to).ok().as_ref() != Some(&wanted) {
            panic!("copying to {}: {error}", to.display());
        }
    }
}

fn run(command: &mut Command) {
    let status = command
        .status()
        .unwrap_or_else(|e| panic!("running {command:?}: {e}"));
    if !status.success() {
        panic!("{command:?} failed with {status}");
    }
}
