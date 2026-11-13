//! On macOS, embeds an Info.plist section (`__TEXT,__info_plist`) in the
//! binary. A program that launchd starts is responsible for its own access
//! to protected folders, so macOS names the host's binary, not Pinrail.app,
//! in the privacy prompts its agents cause; the section makes them name
//! Pinrail.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let version = env::var("CARGO_PKG_VERSION").expect("cargo sets the version");
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>dev.pinrail.host</string>
  <key>CFBundleName</key>
  <string>Pinrail</string>
  <key>CFBundleDisplayName</key>
  <string>Pinrail</string>
  <key>CFBundleShortVersionString</key>
  <string>{version}</string>
  <key>CFBundleVersion</key>
  <string>{version}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
</dict>
</plist>
"#
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("Info.plist");
    fs::write(&out, plist).expect("the Info.plist is written");
    println!(
        "cargo::rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        out.display()
    );
}
