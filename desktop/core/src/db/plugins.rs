//! Installed plugins: what the person has installed, where it came from,
//! and the bundle new reviews use, with the one before it kept for a week
//! so an update can be rolled back.

use rusqlite::{OptionalExtension, params};

use super::Db;

/// How long the release an update replaced is kept for a rollback.
pub const ROLLBACK_DAYS: i64 = 7;

/// One installed plugin: a linked folder served live, or a source whose
/// releases are stored as bundles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRecord {
    /// The full name, `<publisher>/<name>`.
    pub plugin: String,
    pub publisher: String,
    pub name: String,
    /// `bundled`, `folder`, `link`, `git` or `release`
    pub kind: String,
    pub source: String,
    pub resolved: String,
    pub commit: Option<String>,
    pub asset_hash: Option<String>,
    pub build_log: Option<String>,
    /// The bundle new reviews use; none for a link, which is served live.
    pub bundle: Option<String>,
    /// The bundle an update replaced, and until when it is kept.
    pub previous: Option<String>,
    pub previous_until: Option<String>,
    /// For a link that takes a published plugin's place: that
    /// installation, as JSON, to put back when the link is removed.
    pub replaced: Option<String>,
    pub installed_at: String,
    pub updated_at: String,
}

impl InstallRecord {
    pub fn linked(&self) -> bool {
        self.kind == "link"
    }
}

const INSTALL_COLUMNS: &str = "plugin, publisher, name, source_kind, source, resolved, commit_id, \
     asset_hash, build_log, bundle, previous, previous_until, replaced, installed_at, updated_at";

fn install_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<InstallRecord> {
    Ok(InstallRecord {
        plugin: r.get(0)?,
        publisher: r.get(1)?,
        name: r.get(2)?,
        kind: r.get(3)?,
        source: r.get(4)?,
        resolved: r.get(5)?,
        commit: r.get(6)?,
        asset_hash: r.get(7)?,
        build_log: r.get(8)?,
        bundle: r.get(9)?,
        previous: r.get(10)?,
        previous_until: r.get(11)?,
        replaced: r.get(12)?,
        installed_at: r.get(13)?,
        updated_at: r.get(14)?,
    })
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Db {
    /// Every installed plugin, by full name.
    pub fn installs(&self) -> rusqlite::Result<Vec<InstallRecord>> {
        let conn = self.conn();
        conn.prepare(&format!(
            "SELECT {INSTALL_COLUMNS} FROM plugin_installs ORDER BY plugin"
        ))?
        .query_map([], install_row)?
        .collect()
    }

    pub fn install(&self, plugin: &str) -> rusqlite::Result<Option<InstallRecord>> {
        let conn = self.conn();
        conn.query_row(
            &format!("SELECT {INSTALL_COLUMNS} FROM plugin_installs WHERE plugin = ?1"),
            params![plugin],
            install_row,
        )
        .optional()
    }

    /// Writes the installation, replacing the plugin's previous one but
    /// keeping when it was first installed. A bundle that replaces another
    /// keeps the one it replaces for [`ROLLBACK_DAYS`], to roll back to.
    pub fn record_install(&self, record: &InstallRecord) -> rusqlite::Result<()> {
        let conn = self.conn();
        let until = (chrono::Utc::now() + chrono::Duration::days(ROLLBACK_DAYS))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        conn.execute(
            &format!(
                "INSERT INTO plugin_installs ({INSTALL_COLUMNS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, NULL, ?14, ?11, ?12)
                 ON CONFLICT(plugin) DO UPDATE SET
                   source_kind = excluded.source_kind, source = excluded.source,
                   resolved = excluded.resolved, commit_id = excluded.commit_id,
                   asset_hash = excluded.asset_hash, build_log = excluded.build_log,
                   previous = CASE
                     WHEN plugin_installs.bundle IS NOT NULL AND excluded.bundle IS NOT NULL
                          AND plugin_installs.bundle <> excluded.bundle
                     THEN plugin_installs.bundle ELSE plugin_installs.previous END,
                   previous_until = CASE
                     WHEN plugin_installs.bundle IS NOT NULL AND excluded.bundle IS NOT NULL
                          AND plugin_installs.bundle <> excluded.bundle
                     THEN ?13 ELSE plugin_installs.previous_until END,
                   bundle = excluded.bundle,
                   publisher = excluded.publisher,
                   replaced = excluded.replaced,
                   updated_at = excluded.updated_at"
            ),
            params![
                record.plugin,
                record.publisher,
                record.name,
                record.kind,
                record.source,
                record.resolved,
                record.commit,
                record.asset_hash,
                record.build_log,
                record.bundle,
                record.installed_at,
                record.updated_at,
                until,
                record.replaced
            ],
        )?;
        Ok(())
    }

    /// Makes the installation's previous bundle its current again, while it
    /// is kept; the bundle it rolls back to, or none when there is nothing to.
    pub fn roll_back(&self, plugin: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.conn();
        conn.query_row(
            "UPDATE plugin_installs
             SET bundle = previous, previous = NULL, previous_until = NULL, updated_at = ?2
             WHERE plugin = ?1 AND previous IS NOT NULL AND previous_until > ?2
             RETURNING bundle",
            params![plugin, now()],
            |r| r.get(0),
        )
        .optional()
    }

    pub fn remove_install(&self, plugin: &str) -> rusqlite::Result<bool> {
        let conn = self.conn();
        Ok(conn.execute(
            "DELETE FROM plugin_installs WHERE plugin = ?1",
            params![plugin],
        )? > 0)
    }
}
