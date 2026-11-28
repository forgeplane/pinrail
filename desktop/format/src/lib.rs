//! The Pinrail plugin format: what makes a folder a plugin, and what the app
//! makes of it. The manifest and its features, the payload, decision and
//! settings schemas, the samples, the summary declaration and the files a
//! plugin takes beside a payload. The app loads plugins with it, and the CLI
//! checks a folder with it without the app, so both give the same verdict.

use serde::{Deserialize, Serialize};

pub mod attachments;
pub mod bundle;
pub mod compat;
pub mod fixtures;
pub mod manifest;
pub mod markdown;
pub mod sample;
pub mod schema;
pub mod skill;
pub mod summary;

pub use manifest::{Install, Plugin};

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

/// A symbolic link at `link` to the file `target`, for the tests: Windows
/// makes a link to a file apart from one to a folder.
#[cfg(test)]
/// Whether it was made: Windows makes links only with Developer Mode on, or
/// for an administrator, and a test without one says so and goes no further.
#[must_use]
pub(crate) fn file_link(
    target: impl AsRef<std::path::Path>,
    link: impl AsRef<std::path::Path>,
) -> bool {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(target, link);
    match made {
        Ok(()) => true,
        // ERROR_PRIVILEGE_NOT_HELD
        Err(error) if cfg!(windows) && error.raw_os_error() == Some(1314) => {
            eprintln!("skipped: making a link needs Developer Mode on Windows");
            false
        }
        Err(error) => panic!("{error}"),
    }
}
