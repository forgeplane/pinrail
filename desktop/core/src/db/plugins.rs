//! Installed plugins and their lines: what the person has installed and
//! where it came from, and for each line of a plugin the bundle that is
//! current. A line stays while a review renders with it, after its plugin
//! is removed.

use rusqlite::{OptionalExtension, params};

use super::Db;

/// One installed plugin: a linked folder served live, or a source whose
/// releases are stored as bundles on lines.
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
    /// The line new reviews use; none for a link.
    pub line: Option<String>,
    pub installed_at: String,
    pub updated_at: String,
}

impl InstallRecord {
    pub fn linked(&self) -> bool {
        self.kind == "link"
    }
}

/// A line of a plugin and its current bundle, with the one it replaced
/// while that can still be rolled back to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineRecord {
    pub plugin: String,
    pub line: String,
    pub bundle: String,
    pub previous: Option<String>,
    pub previous_until: Option<String>,
}

/// How long the release an update replaced is kept for a rollback.
pub const ROLLBACK_DAYS: i64 = 7;

const INSTALL_COLUMNS: &str = "plugin, publisher, name, source_kind, source, resolved, commit_id, \
     asset_hash, build_log, line, installed_at, updated_at";

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
        line: r.get(9)?,
        installed_at: r.get(10)?,
        updated_at: r.get(11)?,
    })
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

    /// Every line of every plugin, installed or not.
    pub fn lines(&self) -> rusqlite::Result<Vec<LineRecord>> {
        let conn = self.conn();
        conn.prepare(
            "SELECT plugin, line, bundle, previous, previous_until FROM plugin_lines
             ORDER BY plugin, line",
        )?
        .query_map([], |r| {
            Ok(LineRecord {
                plugin: r.get(0)?,
                line: r.get(1)?,
                bundle: r.get(2)?,
                previous: r.get(3)?,
                previous_until: r.get(4)?,
            })
        })?
        .collect()
    }

    /// Writes the installation, replacing the plugin's previous one but
    /// keeping when it was first installed, and makes `current` the bundle
    /// of its line: both or neither. The bundle it replaces on the line is
    /// kept for [`ROLLBACK_DAYS`], to roll back to.
    pub fn record_install(
        &self,
        record: &InstallRecord,
        current: Option<(&str, &str)>,
    ) -> rusqlite::Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            &format!(
                "INSERT INTO plugin_installs ({INSTALL_COLUMNS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(plugin) DO UPDATE SET
                   source_kind = excluded.source_kind, source = excluded.source,
                   resolved = excluded.resolved, commit_id = excluded.commit_id,
                   asset_hash = excluded.asset_hash, build_log = excluded.build_log,
                   line = excluded.line, updated_at = excluded.updated_at"
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
                record.line,
                record.installed_at,
                record.updated_at
            ],
        )?;
        if let Some((line, bundle)) = current {
            let until = (chrono::Utc::now() + chrono::Duration::days(ROLLBACK_DAYS))
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            tx.execute(
                "INSERT INTO plugin_lines (plugin, line, bundle) VALUES (?1, ?2, ?3)
                 ON CONFLICT(plugin, line) DO UPDATE SET
                   previous = CASE WHEN plugin_lines.bundle <> excluded.bundle
                     THEN plugin_lines.bundle ELSE plugin_lines.previous END,
                   previous_until = CASE WHEN plugin_lines.bundle <> excluded.bundle
                     THEN ?4 ELSE plugin_lines.previous_until END,
                   bundle = excluded.bundle",
                params![record.plugin, line, bundle, until],
            )?;
        }
        tx.commit()
    }

    /// Whether any review renders with this line of the plugin.
    pub fn reviews_use(&self, plugin: &str, line: &str) -> rusqlite::Result<bool> {
        let conn = self.conn();
        conn.query_row(
            "SELECT 1 FROM reviews WHERE plugin = ?1 AND plugin_line = ?2 LIMIT 1",
            params![plugin, line],
            |_| Ok(()),
        )
        .optional()
        .map(|r| r.is_some())
    }

    /// Makes the line's previous bundle its current again, while it is kept;
    /// the bundle it rolls back to, or none when there is nothing to.
    pub fn roll_back(&self, plugin: &str, line: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.conn();
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        conn.query_row(
            "UPDATE plugin_lines
             SET bundle = previous, previous = NULL, previous_until = NULL
             WHERE plugin = ?1 AND line = ?2 AND previous IS NOT NULL AND previous_until > ?3
             RETURNING bundle",
            params![plugin, line, now],
            |r| r.get(0),
        )
        .optional()
    }

    pub fn remove_line(&self, plugin: &str, line: &str) -> rusqlite::Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM plugin_lines WHERE plugin = ?1 AND line = ?2",
            params![plugin, line],
        )?;
        Ok(())
    }

    pub fn remove_install(&self, plugin: &str) -> rusqlite::Result<bool> {
        let conn = self.conn();
        Ok(conn.execute(
            "DELETE FROM plugin_installs WHERE plugin = ?1",
            params![plugin],
        )? > 0)
    }
}
