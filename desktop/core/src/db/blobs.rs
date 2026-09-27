//! Blobs: a file's bytes stored once by their SHA-256, whatever reviews
//! carry them.

use chrono::{SecondsFormat, Utc};
use rusqlite::{OptionalExtension, params};

use super::Db;

impl Db {
    /// The size of a stored blob, or none.
    pub fn blob_size(&self, sha256: &str) -> rusqlite::Result<Option<u64>> {
        let conn = self.conn();
        conn.query_row(
            "SELECT size FROM blobs WHERE sha256 = ?1",
            params![sha256],
            |r| r.get::<_, i64>(0),
        )
        .optional()
        .map(|size| size.map(|s| s as u64))
    }

    /// Records a blob whose file is in place; a second record of the same
    /// content changes nothing.
    pub fn insert_blob(&self, sha256: &str, size: u64) -> rusqlite::Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT OR IGNORE INTO blobs (sha256, size, created_at) VALUES (?1, ?2, ?3)",
            params![
                sha256,
                size as i64,
                Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
            ],
        )?;
        Ok(())
    }

    /// Blobs no review names, stored before `before`.
    pub fn orphan_blobs(&self, before: chrono::DateTime<Utc>) -> rusqlite::Result<Vec<String>> {
        let conn = self.conn();
        conn.prepare(
            "SELECT b.sha256 FROM blobs b
             WHERE b.created_at < ?1
               AND NOT EXISTS (SELECT 1 FROM review_attachments a WHERE a.sha256 = b.sha256)",
        )?
        .query_map(
            params![before.to_rfc3339_opts(SecondsFormat::Secs, true)],
            |r| r.get(0),
        )?
        .collect()
    }

    pub fn delete_blob(&self, sha256: &str) -> rusqlite::Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM blobs WHERE sha256 = ?1", params![sha256])?;
        Ok(())
    }

    /// How many blobs are stored, and how many bytes they add up to.
    pub fn blob_totals(&self) -> rusqlite::Result<(u64, u64)> {
        let conn = self.conn();
        conn.query_row(
            "SELECT count(*), coalesce(sum(size), 0) FROM blobs",
            [],
            |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
        )
    }
}
