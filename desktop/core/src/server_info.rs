//! `<data dir>/server.json`: how the CLI finds a running server. Written when
//! the server starts listening, removed when it stops.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::Config;

pub const FILE_NAME: &str = "server.json";

pub fn path(config: &Config) -> PathBuf {
    config.data_dir.join(FILE_NAME)
}

pub fn write(config: &Config, started_at: DateTime<Utc>) -> std::io::Result<()> {
    std::fs::create_dir_all(&config.data_dir)?;
    let info = json!({
        "url": config.url(),
        "port": config.port,
        "pid": std::process::id(),
        "started_at": crate::reviews::iso(started_at),
    });
    let mut body = info.to_string();
    body.push('\n');
    std::fs::write(path(config), body)
}

/// Removes the file if it still advertises this process.
pub fn remove(config: &Config) {
    let file = path(config);
    if advertises_this_process(&file) {
        let _ = std::fs::remove_file(file);
    }
}

fn advertises_this_process(file: &Path) -> bool {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
        .and_then(|v| v["pid"].as_u64())
        .is_some_and(|pid| pid == std::process::id() as u64)
}
