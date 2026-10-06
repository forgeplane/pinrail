// The JSON Schemas of the manifest and of a reference to an attached file
// belong to the pinrail-sdk package, where authors get them. The format
// holds every manifest to the first, and checks a payload schema's file
// fields against the second.

use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    for name in ["manifest.schema.json", "attachment.schema.json"] {
        let schema = manifest.join("../../sdk/schemas").join(name);
        println!("cargo:rerun-if-changed={}", schema.display());
        fs::copy(&schema, out.join(name)).unwrap();
    }
}
