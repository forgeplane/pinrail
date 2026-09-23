//! Blobs: a file's bytes stored once by their SHA-256, whatever reviews
//! carry them.

use chrono::{SecondsFormat, Utc};
use rusqlite::{OptionalExtension, params};

use super::Db;

impl Db {
    /// The size of a stored blob, or none.
    pub fn blob_size(&self, sha256: &str) -> rusqlite::Result<Option<u64>> {
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
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
}
