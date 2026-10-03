//! Installed plugins: what the person has installed, where it came from,
//! and the bundle new reviews use.

use rusqlite::{OptionalExtension, params};

use super::Db;

/// One installed plugin: a linked folder served live, or a source whose
/// releases are stored as bundles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRecord {
    /// The full name, `<publisher>/<name>`.
    pub plugin: String,
    pub publisher: String,
    pub name: String,
    /// `bundled`, `folder`, `archive` or `link`
    pub kind: String,
    pub source: String,
    pub resolved: String,
    /// The bundle new reviews use; none for a link, which is served live.
    pub bundle: Option<String>,
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

const INSTALL_COLUMNS: &str = "plugin, publisher, name, source_kind, source, resolved, bundle, replaced, installed_at, updated_at";

fn install_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<InstallRecord> {
    Ok(InstallRecord {
        plugin: r.get(0)?,
        publisher: r.get(1)?,
        name: r.get(2)?,
        kind: r.get(3)?,
        source: r.get(4)?,
        resolved: r.get(5)?,
        bundle: r.get(6)?,
        replaced: r.get(7)?,
        installed_at: r.get(8)?,
        updated_at: r.get(9)?,
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

    /// Writes the installation, replacing the plugin's previous one but
    /// keeping when it was first installed.
    pub fn record_install(&self, record: &InstallRecord) -> rusqlite::Result<()> {
        let conn = self.conn();
        conn.execute(
            &format!(
                "INSERT INTO plugin_installs ({INSTALL_COLUMNS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(plugin) DO UPDATE SET
                   publisher = excluded.publisher,
                   source_kind = excluded.source_kind, source = excluded.source,
                   resolved = excluded.resolved,
                   bundle = excluded.bundle, replaced = excluded.replaced,
                   updated_at = excluded.updated_at"
            ),
            params![
                record.plugin,
                record.publisher,
                record.name,
                record.kind,
                record.source,
                record.resolved,
                record.bundle,
                record.replaced,
                record.installed_at,
                record.updated_at,
            ],
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
