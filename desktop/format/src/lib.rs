//! The Pinrail plugin format: what makes a folder a plugin, and what the app
//! makes of it. The manifest and its features, the payload, decision and
//! settings schemas, the samples, the summary declaration and the files a
//! plugin takes beside a payload. The app loads plugins with it, and the CLI
//! checks a folder with it without the app, so both give the same verdict.

use serde::{Deserialize, Serialize};

pub mod attachments;
pub mod bundle;
pub mod manifest;
pub mod sample;
pub mod schema;
pub mod summary;

pub use manifest::{Install, Line, Plugin};

/// One thing wrong with a document: where, as a JSON pointer into it, and
/// what.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Violation {
    pub path: String,
    pub message: String,
}

impl Violation {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

/// A version's three numbers, `0` for any that is missing or not a number:
/// enough to compare a plugin's needed Pinrail with this one.
pub fn semver(text: &str) -> (u64, u64, u64) {
    let mut parts = text.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// Checks a plugin's decision template compiles, at load, so a bad one is
/// reported on the plugin's row rather than at the first review.
pub fn compile_template(source: &str) -> Result<(), String> {
    let mut env = minijinja::Environment::new();
    env.add_template("decision", source)
        .map_err(|e| e.to_string())
}
