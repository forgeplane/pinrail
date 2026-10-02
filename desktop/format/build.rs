// The manifest's JSON Schema belongs to the pinrail-plugin package, where
// authors get it; the format holds every manifest to the same file.

use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let schema = manifest.join("../../pinrail-plugin/schemas/manifest.schema.json");
    println!("cargo:rerun-if-changed={}", schema.display());
    fs::copy(&schema, out.join("manifest.schema.json")).unwrap();
}
