//! SQLite: the record of truth. Reviews and their outcomes are
//! written once; events are appended; the one-ending rule is the primary
//! key on `outcomes`.
//!
//! One file, written through one connection and read through a few more,
//! in six areas: the migrations that make the tables, reviews and the
//! queries over them, how a review ends, the event log, the record of
//! installed plugins, and the blobs uploaded beside reviews.

mod blobs;
mod bundles;
mod events;
mod item_work;
mod items;
mod messages;
mod outcomes;
mod plugins;
mod projects;
mod repos;
mod reviews;
mod schema;
mod sessions;

pub use bundles::BundleRecord;
pub use events::Event;
pub use item_work::{Claim, Comment, Edited, Entry, ItemChange, ItemReview, ItemSession, Place};
pub use items::{ItemFilters, ItemTag, Refused, Related, State, WorkItem};
pub use messages::{Deleted, MessageRow, message_notice};
pub use plugins::InstallRecord;
pub use projects::{Change, Details, NotChanged, ProjectRef, ProjectRow, RepoRow};
pub use reviews::{Facets, Filters, NO_PROJECT, NotStored};
pub use schema::LATEST_MIGRATION;
pub use sessions::{
    Added, OpenStage, SessionFilters, SessionRow, StageRow, StatusKind, StatusRow,
    delivered_notice, status_notice,
};

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};

use rusqlite::{Connection, OpenFlags};

use schema::migrate;

/// How many read-only connections a database file is read through.
const READERS: usize = 4;

#[derive(Debug)]
pub struct Db {
    /// the connection every write goes through, one at a time
    conn: Mutex<Connection>,
    /// read-only connections to the same file, which WAL lets read while a
    /// write runs; none for a database in memory, which reads through `conn`
    readers: Vec<Mutex<Connection>>,
    /// the reader to try first, so reads spread over them
    next: AtomicUsize,
}

impl Db {
    /// The connection. A panic while it was held leaves the lock poisoned;
    /// it is taken anyway, since SQLite rolls back the transaction that was
    /// open, so one failed request does not fail every one after it.
    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A connection to read through: a free read-only connection, else
    /// the next one once it is free. A slow read or a write in progress
    /// holds up no other read.
    fn read(&self) -> MutexGuard<'_, Connection> {
        if self.readers.is_empty() {
            return self.conn();
        }
        let start = self.next.fetch_add(1, Ordering::Relaxed);
        for k in 0..self.readers.len() {
            match self.readers[(start + k) % self.readers.len()].try_lock() {
                Ok(reader) => return reader,
                Err(TryLockError::Poisoned(poisoned)) => return poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => {}
            }
        }
        self.readers[start % self.readers.len()]
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn open(path: &Path) -> rusqlite::Result<Db> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
        )?;
        let mut db = Self::init(conn)?;
        // opened once the migrations have made the tables
        for _ in 0..READERS {
            let reader = Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            reader.execute_batch("PRAGMA busy_timeout = 5000;")?;
            db.readers.push(Mutex::new(reader));
        }
        Ok(db)
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
            readers: Vec::new(),
            next: AtomicUsize::new(0),
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
        assert!(db.installs().is_ok());
    }

    /// Whether `read` finishes on another thread within a second.
    fn finishes_soon(db: &std::sync::Arc<Db>, read: fn(&Db) -> bool) -> bool {
        let db = db.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(read(&db));
        });
        rx.recv_timeout(std::time::Duration::from_secs(1)) == Ok(true)
    }

    #[test]
    fn a_read_does_not_wait_for_a_write_or_another_read_in_progress() {
        let dir = tempfile::tempdir().unwrap();
        let db = std::sync::Arc::new(Db::open(&dir.path().join("pinrail.db")).unwrap());
        // a long write holds the write connection
        let write = db.conn();
        assert!(finishes_soon(&db, |db| db.installs().is_ok()));
        // and a long read one of the read connections
        let read = db.read();
        assert!(finishes_soon(&db, |db| db.installs().is_ok()));
        drop((write, read));
    }
}
