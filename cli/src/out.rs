//! Output: JSON on stdout, diagnostics on stderr, the decision file.

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

pub fn print_json(value: &Value, pretty: bool) {
    let text = if pretty {
        serde_json::to_string_pretty(value).unwrap_or_default()
    } else {
        serde_json::to_string(value).unwrap_or_default()
    };
    println!("{text}");
}

/// A refused request: the server's error body, pretty, on stderr.
pub fn error_json(body: &Value) {
    match body {
        Value::Null => eprintln!("wicket: the server refused the request"),
        other => eprintln!(
            "wicket: {}",
            serde_json::to_string_pretty(other).unwrap_or_default()
        ),
    }
}

/// `decision.data` as a 2-space-indented file with a trailing newline, the
/// shape the workflows' publish steps read.
pub fn write_decision(path: &Path, data: &Value) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut text = serde_json::to_string_pretty(data)?;
    text.push('\n');
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}
