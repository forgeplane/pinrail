//! SQLite: the record of truth. Reviews and their outcomes are
//! written once; events are appended; the one-decision rule is the primary
//! key on `decisions`.
//!
//! One connection and one file, in five areas: the tables and the numbered
//! steps that migrate them, reviews and the queries over them, how a review
//! ends, the event log, and the record of installed plugins.

mod events;
mod outcomes;
mod plugins;
mod reviews;
mod schema;

pub use events::Event;
pub use plugins::InstalledRecord;
pub use reviews::{Facets, Filters, NO_PROJECT};
pub use schema::SCHEMA_VERSION;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use schema::migrate;

#[derive(Debug)]
pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Db> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
        )?;
        Self::init(conn)
    }

    pub fn in_memory() -> rusqlite::Result<Db> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> rusqlite::Result<Db> {
        migrate(&conn)?;
        Ok(Db {
            conn: Mutex::new(conn),
        })
    }
}
