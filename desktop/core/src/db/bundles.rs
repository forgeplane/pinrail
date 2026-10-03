//! Plugin bundles: one release of a plugin as files, stored once by the hash
//! of its listing, whatever installs it.

use std::collections::HashSet;

use chrono::{SecondsFormat, Utc};
use rusqlite::{OptionalExtension, params};

use super::Db;

/// A stored bundle, as its row records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleRecord {
    pub hash: String,
    pub name: String,
    pub version: String,
    /// The manifest as the bundle holds it.
    pub manifest: String,
    pub size: u64,
    pub stored_at: String,
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

impl Db {
    /// Records a bundle whose folder is in place, with its listing. A second
    /// record of the same bundle only makes it new again, for the sweep.
    pub fn insert_bundle(&self, record: &BundleRecord, listing: &str) -> rusqlite::Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO plugin_bundles (hash, name, version, manifest, size, stored_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(hash) DO UPDATE SET stored_at = excluded.stored_at",
            params![
                record.hash,
                record.name,
                record.version,
                record.manifest,
                record.size as i64,
                now()
            ],
        )?;
        tx.execute(
            "INSERT INTO plugin_bundle_files (hash, listing) VALUES (?1, ?2)
             ON CONFLICT(hash) DO NOTHING",
            params![record.hash, listing],
        )?;
        tx.commit()
    }

    /// Makes a stored bundle new again, for the sweep. Returns whether it
    /// was there.
    pub fn touch_bundle(&self, hash: &str) -> rusqlite::Result<bool> {
        let conn = self.conn();
        let touched = conn.execute(
            "UPDATE plugin_bundles SET stored_at = ?2 WHERE hash = ?1",
            params![hash, now()],
        )?;
        Ok(touched > 0)
    }

    pub fn bundle(&self, hash: &str) -> rusqlite::Result<Option<BundleRecord>> {
        let conn = self.conn();
        conn.query_row(
            "SELECT hash, name, version, manifest, size, stored_at
             FROM plugin_bundles WHERE hash = ?1",
            params![hash],
            |r| {
                Ok(BundleRecord {
                    hash: r.get(0)?,
                    name: r.get(1)?,
                    version: r.get(2)?,
                    manifest: r.get(3)?,
                    size: r.get::<_, i64>(4)? as u64,
                    stored_at: r.get(5)?,
                })
            },
        )
        .optional()
    }

    /// The listing a stored bundle's hash was taken over.
    pub fn bundle_listing(&self, hash: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.conn();
        conn.query_row(
            "SELECT listing FROM plugin_bundle_files WHERE hash = ?1",
            params![hash],
            |r| r.get(0),
        )
        .optional()
    }

    /// Every stored bundle's hash.
    pub fn bundle_hashes(&self) -> rusqlite::Result<HashSet<String>> {
        let conn = self.conn();
        conn.prepare("SELECT hash FROM plugin_bundles")?
            .query_map([], |r| r.get(0))?
            .collect()
    }

    /// Deletes the bundles stored before `before` that nothing refers to:
    /// no review was submitted to them, and no installation uses them.
    /// Returns the ones it deleted.
    pub fn delete_unreferenced_bundles(
        &self,
        before: chrono::DateTime<Utc>,
    ) -> rusqlite::Result<Vec<String>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let gone = tx
            .prepare(
                "DELETE FROM plugin_bundles
                 WHERE stored_at < ?1
                   AND NOT EXISTS (SELECT 1 FROM plugin_installs i
                                   WHERE i.bundle = plugin_bundles.hash)
                   AND NOT EXISTS (SELECT 1 FROM reviews r
                                   WHERE r.plugin_bundle = plugin_bundles.hash)
                 RETURNING hash",
            )?
            .query_map(
                params![before.to_rfc3339_opts(SecondsFormat::Secs, true)],
                |r| r.get(0),
            )?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        tx.commit()?;
        Ok(gone)
    }
}
