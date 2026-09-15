//! Installing a plugin from a folder: inspect it, place its files in the
//! store, record where it came from. A link points the registry at the
//! folder instead and serves it live.
//!
//! The store keeps one entry per plugin and major version, the latest
//! installed in that line: an equal or higher version replaces it, an
//! older one is refused unless forced. An old line stays while a review
//! still renders from it.

use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::db::{Db, InstalledRecord};
use crate::error::Error;
use crate::plugins::{Plugin, Registry, copy_dir, hash_dir};

pub struct Options {
    /// serve the folder live instead of copying it
    pub link: bool,
    /// replace a newer version already installed
    pub force: bool,
}

/// Installs the plugin at `source`. Returns its record; the registry has
/// been reloaded with it.
pub fn install_path(
    db: &Db,
    registry: &Registry,
    source: &Path,
    options: Options,
) -> Result<InstalledRecord, Error> {
    let dir = std::path::absolute(source)?;
    if !dir.is_dir() {
        return Err(Error::invalid(
            "/source",
            format!("{} is not a directory", dir.display()),
        ));
    }
    let plugin = Plugin::load(&dir);
    if let Some(why) = &plugin.error {
        return Err(Error::invalid("/source", format!("not a plugin: {why}")));
    }
    if plugin.manifest.get("build").is_some_and(|b| !b.is_null()) {
        return Err(Error::invalid(
            "/source",
            "the manifest declares a build; building on install is not here yet — build it, then install the folder",
        ));
    }

    // what is there already, and whether the new one may take its place
    let records = db.installed_plugins()?;
    let existing = records.iter().find(|r| r.name == plugin.name);
    if let Some(current) = existing
        && !current.linked
        && !options.link
        && current.major == plugin.version as i64
        && semver(&plugin.release) < semver(&current.version)
        && !options.force
    {
        return Err(Error::invalid(
            "/source",
            format!(
                "{} {} is older than the installed {}; pass force to replace it",
                plugin.name, plugin.release, current.version
            ),
        ));
    }

    let now = crate::review::iso(Utc::now());
    let record = if options.link {
        InstalledRecord {
            name: plugin.name.clone(),
            version: plugin.release.clone(),
            major: plugin.version as i64,
            kind: "path".into(),
            source: source.display().to_string(),
            resolved: dir.display().to_string(),
            commit: None,
            asset_hash: None,
            hash: None,
            build_log: None,
            installed_at: now,
            linked: true,
            path: dir.display().to_string(),
        }
    } else {
        let entry = place(registry, &plugin, &dir)?;
        let hash = hash_dir(&entry)?;
        InstalledRecord {
            name: plugin.name.clone(),
            version: plugin.release.clone(),
            major: plugin.version as i64,
            kind: "path".into(),
            source: source.display().to_string(),
            resolved: dir.display().to_string(),
            commit: None,
            asset_hash: None,
            hash: Some(hash),
            build_log: None,
            installed_at: now,
            linked: false,
            path: entry.display().to_string(),
        }
    };

    // the previous line goes when no review renders from it any more
    if let Some(current) = existing
        && !current.linked
        && current.major != record.major
        && !db.reviews_use(&current.name, current.major as u32)?
    {
        let _ = std::fs::remove_dir_all(registry.store_entry(&current.name, current.major));
    }

    db.upsert_installed(&record)?;
    let records = db.installed_plugins()?;
    registry
        .reload_with(records)
        .map_err(|message| Error::invalid("/source", message))?;
    Ok(record)
}

/// Copies the plugin's files into the store entry for its line, whole or
/// not at all: the copy lands beside the entry and takes its place with
/// one rename.
fn place(registry: &Registry, plugin: &Plugin, dir: &Path) -> Result<PathBuf, Error> {
    let entry = registry.store_entry(&plugin.name, plugin.version as i64);
    let staging = entry.with_extension("staging");
    let _ = std::fs::remove_dir_all(&staging);
    if let Some(parent) = entry.parent() {
        std::fs::create_dir_all(parent)?;
    }
    copy_dir(dir, &staging)?;
    if entry.exists() {
        let old = entry.with_extension("old");
        let _ = std::fs::remove_dir_all(&old);
        std::fs::rename(&entry, &old)?;
        std::fs::rename(&staging, &entry)?;
        let _ = std::fs::remove_dir_all(&old);
    } else {
        std::fs::rename(&staging, &entry)?;
    }
    Ok(entry)
}

/// "1.2.3" as something that orders; anything else sorts first.
pub fn semver(text: &str) -> (u64, u64, u64) {
    let mut parts = text.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}
