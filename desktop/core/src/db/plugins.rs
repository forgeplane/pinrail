//! Installed plugins: what the person has installed, where it came from,
//! and the bundle new reviews use.

use rusqlite::{OptionalExtension, params};

use super::Db;

/// One installed plugin: a folder or a zip from disk, or a plugin the app
/// carries, under its manifest's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRecord {
    pub name: String,
    /// `app`, `folder` or `archive`
    pub kind: String,
    /// The folder or the zip, as a full path; empty for `app`.
    pub source: String,
    /// The folder is followed, not copied.
    pub link: bool,
    /// The bundle new reviews use: for a link, its folder as last captured.
    pub bundle: String,
    pub installed_at: String,
    pub updated_at: String,
}

impl InstallRecord {
    pub fn linked(&self) -> bool {
        self.link
    }
}

const INSTALL_COLUMNS: &str = "name, source_kind, source, link, bundle, installed_at, updated_at";

fn install_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<InstallRecord> {
    Ok(InstallRecord {
        name: r.get(0)?,
        kind: r.get(1)?,
        source: r.get(2)?,
        link: r.get(3)?,
        bundle: r.get(4)?,
        installed_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

impl Db {
    /// Every installed plugin, by full name.
    pub fn installs(&self) -> rusqlite::Result<Vec<InstallRecord>> {
        let conn = self.read();
        conn.prepare(&format!(
            "SELECT {INSTALL_COLUMNS} FROM plugin_installs ORDER BY name"
        ))?
        .query_map([], install_row)?
        .collect()
    }

    pub fn install(&self, name: &str) -> rusqlite::Result<Option<InstallRecord>> {
        let conn = self.read();
        conn.query_row(
            &format!("SELECT {INSTALL_COLUMNS} FROM plugin_installs WHERE name = ?1"),
            params![name],
            install_row,
        )
        .optional()
    }

    /// Writes the installation, replacing the one under its name but
    /// keeping when that was first installed.
    pub fn record_install(&self, record: &InstallRecord) -> rusqlite::Result<()> {
        let conn = self.conn();
        // a skill allowed from one source is not allowed from another
        conn.execute(
            "DELETE FROM plugin_skill_grants WHERE name = ?1 AND EXISTS (
               SELECT 1 FROM plugin_installs
               WHERE name = ?1 AND (source_kind != ?2 OR source != ?3))",
            params![record.name, record.kind, record.source],
        )?;
        conn.execute(
            &format!(
                "INSERT INTO plugin_installs ({INSTALL_COLUMNS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(name) DO UPDATE SET
                   source_kind = excluded.source_kind, source = excluded.source,
                   link = excluded.link, bundle = excluded.bundle,
                   updated_at = excluded.updated_at"
            ),
            params![
                record.name,
                record.kind,
                record.source,
                record.link,
                record.bundle,
                record.installed_at,
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn remove_install(&self, name: &str) -> rusqlite::Result<bool> {
        let conn = self.conn();
        Ok(conn.execute("DELETE FROM plugin_installs WHERE name = ?1", params![name])? > 0)
    }

    /// Allows the skill of an installed plugin to reach the agents, or
    /// stops it; whether the plugin is installed.
    pub fn allow_skill(&self, name: &str, allowed: bool, at: &str) -> rusqlite::Result<bool> {
        let conn = self.conn();
        let installed: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM plugin_installs WHERE name = ?1)",
            params![name],
            |r| r.get(0),
        )?;
        if !installed {
            return Ok(false);
        }
        if allowed {
            conn.execute(
                "INSERT INTO plugin_skill_grants (name, granted_at) VALUES (?1, ?2)
                 ON CONFLICT(name) DO NOTHING",
                params![name, at],
            )?;
        } else {
            conn.execute(
                "DELETE FROM plugin_skill_grants WHERE name = ?1",
                params![name],
            )?;
        }
        Ok(true)
    }

    /// The installed plugins whose skill the person allowed.
    pub fn skill_grants(&self) -> rusqlite::Result<std::collections::HashSet<String>> {
        let conn = self.read();
        conn.prepare("SELECT name FROM plugin_skill_grants")?
            .query_map([], |r| r.get(0))?
            .collect()
    }
}
