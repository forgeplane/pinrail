//! Repositories: a row for each name an origin, a session or a project gave,
//! such as acme/api. Reviews and sessions refer to their repository's row,
//! so deleting the row deletes them, and the row holds the repository's
//! project.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

/// The id of the repository named `name`, adding its row if it has none;
/// `None` for no name or an empty one. Two requests naming a new
/// repository at once share one row.
pub(super) fn ensure(
    conn: &Connection,
    name: Option<&str>,
    at: DateTime<Utc>,
) -> rusqlite::Result<Option<i64>> {
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        return Ok(None);
    };
    conn.execute(
        "INSERT INTO repos (name, created_at) VALUES (?1, ?2) ON CONFLICT (name) DO NOTHING",
        params![name, crate::reviews::iso(at)],
    )?;
    conn.query_row(
        "SELECT id FROM repos WHERE name = ?1",
        params![name],
        |row| row.get(0),
    )
    .map(Some)
}

/// The SQL for the name of the repository whose id is the column `id`.
pub(super) fn name_of(id: &str) -> String {
    format!("(SELECT name FROM repos WHERE id = {id})")
}

/// The SQL that keeps rows whose repository id, the column `id`, is the
/// repository named by the parameter `?n`.
pub(super) fn named(id: &str, n: usize) -> String {
    format!("{id} = (SELECT id FROM repos WHERE name = ?{n})")
}
