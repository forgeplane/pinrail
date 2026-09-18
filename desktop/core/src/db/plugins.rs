//! Installed plugins: what the app has, where it came from, and whether a
//! review still renders from a version that would otherwise be removed.

use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use serde_json::Value;

use super::Db;

/// One installed plugin: a linked folder served live, or an entry in the
/// store, with where it came from and what was placed.
#[derive(Debug, Clone)]
pub struct InstalledRecord {
    pub name: String,
    pub version: String,
    pub major: i64,
    /// `path`, `git` or `release`
    pub kind: String,
    pub source: String,
    pub resolved: String,
    pub commit: Option<String>,
    pub asset_hash: Option<String>,
    /// SHA-256 over the placed files; none for a link
    pub hash: Option<String>,
    pub build_log: Option<String>,
    pub installed_at: String,
    pub linked: bool,
    /// the folder for a link, the store entry otherwise
    pub path: String,
}

impl InstalledRecord {
    /// A link to a plugin folder as it is: the manifest names it.
    pub fn linked(dir: &std::path::Path) -> Option<InstalledRecord> {
        let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
        let manifest: Value = serde_json::from_str(&text).ok()?;
        let name = manifest.get("name")?.as_str()?.to_string();
        let (version, major) = crate::plugins::version_of(manifest.get("version")?)?;
        let path = std::path::absolute(dir).ok()?.display().to_string();
        Some(InstalledRecord {
            name,
            version,
            major,
            kind: "path".into(),
            source: path.clone(),
            resolved: path.clone(),
            commit: None,
            asset_hash: None,
            hash: None,
            build_log: None,
            installed_at: crate::reviews::iso(Utc::now()),
            linked: true,
            path,
        })
    }
}

impl Db {
    pub fn installed_plugins(&self) -> rusqlite::Result<Vec<InstalledRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name, version, major, kind, source, resolved, commit_id, asset_hash, hash, build_log, installed_at, linked, path
             FROM installed_plugins ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(InstalledRecord {
                name: r.get(0)?,
                version: r.get(1)?,
                major: r.get(2)?,
                kind: r.get(3)?,
                source: r.get(4)?,
                resolved: r.get(5)?,
                commit: r.get(6)?,
                asset_hash: r.get(7)?,
                hash: r.get(8)?,
                build_log: r.get(9)?,
                installed_at: r.get(10)?,
                linked: r.get::<_, i64>(11)? != 0,
                path: r.get(12)?,
            })
        })?;
        rows.collect()
    }

    /// Writes the record, replacing the plugin's previous one.
    pub fn upsert_installed(&self, record: &InstalledRecord) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO installed_plugins
             (name, version, major, kind, source, resolved, commit_id, asset_hash, hash, build_log, installed_at, linked, path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.name,
                record.version,
                record.major,
                record.kind,
                record.source,
                record.resolved,
                record.commit,
                record.asset_hash,
                record.hash,
                record.build_log,
                record.installed_at,
                record.linked as i64,
                record.path
            ],
        )?;
        Ok(())
    }

    /// Whether any review renders from this plugin's line.
    pub fn reviews_use(&self, plugin: &str, major: u32) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT 1 FROM reviews WHERE plugin = ?1 AND plugin_version = ?2 LIMIT 1",
            params![plugin, major],
            |_| Ok(()),
        )
        .optional()
        .map(|r| r.is_some())
    }

    pub fn remove_installed(&self, name: &str) -> rusqlite::Result<bool> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute(
            "DELETE FROM installed_plugins WHERE name = ?1",
            params![name],
        )? > 0)
    }
}
