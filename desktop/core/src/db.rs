//! SQLite: the record of truth. Reviews and their outcomes are
//! written once; events are appended; the one-ending rule is the primary
//! key on `outcomes`.
//!
//! One connection and one file, in six areas: the migrations that
//! make the tables, reviews and the queries over them, how a review
//! ends, the event log, the record of installed plugins, and the blobs
//! uploaded beside reviews.

mod blobs;
mod events;
mod outcomes;
mod plugins;
mod reviews;
mod schema;

pub use events::Event;
pub use plugins::InstalledRecord;
pub use reviews::{Facets, Filters, NO_PROJECT, NotStored};
pub use schema::LATEST_MIGRATION;

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::Connection;

use schema::migrate;

#[derive(Debug)]
pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// The connection. A panic while it was held leaves the lock poisoned;
    /// it is taken anyway, since SQLite rolls back the transaction that was
    /// open, so one failed request does not fail every one after it.
    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_while_the_database_is_in_use_does_not_take_it_down() {
        // one request that panics mid-query must not make every later one
        // panic until the app restarts
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("pinrail.db")).unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = db.conn.lock().unwrap();
            panic!("a request fails while it holds the connection");
        }));
        assert!(db.installed_plugins().is_ok());
    }
}
