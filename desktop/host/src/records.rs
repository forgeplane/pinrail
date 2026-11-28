//! `terminals.json`: a record of each live or recently ended terminal, so
//! that `pinrail-host status` and a restarted core can tell what runs. It
//! is written by one background writer at most once a second, except a new
//! terminal's record, which is written before its program starts, and the
//! last write, which the host makes as it stops.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::Notify;

pub struct Records {
    path: PathBuf,
    entries: Mutex<BTreeMap<String, Value>>,
    changed: Notify,
    /// Held while the file is written: the writer and a new terminal's
    /// record share one temporary file.
    writing: Mutex<()>,
}

impl Records {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            entries: Mutex::new(BTreeMap::new()),
            changed: Notify::new(),
            writing: Mutex::new(()),
        }
    }

    /// The records at `path`, with those a previous host wrote, so that a
    /// host that starts can tell the terminals it adopts.
    pub fn open(path: PathBuf) -> Self {
        let records = Self::new(path);
        let written: Vec<Value> = std::fs::read_to_string(&records.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        {
            let mut entries = records.entries.lock().unwrap_or_else(|e| e.into_inner());
            for record in written {
                if let Some(id) = record["id"].as_str() {
                    entries.insert(id.to_string(), record.clone());
                }
            }
        }
        records
    }

    /// A terminal's record.
    pub fn get(&self, id: &str) -> Option<Value> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
    }

    /// Records a terminal and writes the file now.
    pub fn insert_now(&self, id: &str, record: Value) -> io::Result<()> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), record);
        self.write()
    }

    /// Changes a terminal's record; the writer writes it within a second.
    pub fn update(&self, id: &str, change: impl FnOnce(&mut Value)) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(record) = entries.get_mut(id) {
            change(record);
            self.changed.notify_one();
        }
    }

    /// Gives `ended_at` to the records without one that `held` does not
    /// name: terminals that a host stopped or lost without recording it,
    /// such as a host that was killed. They have no exit. A host that
    /// starts calls it once it has adopted what the holders keep, so that
    /// these records are forgotten like the others.
    pub fn end_lost(&self, held: impl Fn(&str) -> bool, ended_at: &str) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut changed = false;
        for (id, record) in entries.iter_mut() {
            if record["ended_at"].is_null() && !held(id) {
                record["ended_at"] = Value::from(ended_at);
                changed = true;
            }
        }
        if changed {
            self.changed.notify_one();
        }
    }

    /// Forgets the ended terminals that `held` does not name, keeping the
    /// newest `keep` of those that ended at or after `cutoff`, and returns
    /// the ids forgotten. A host that starts calls it for the terminals
    /// earlier hosts ended, which it does not hold.
    pub fn forget_ended(
        &self,
        held: impl Fn(&str) -> bool,
        cutoff: &str,
        keep: usize,
    ) -> Vec<String> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut ended: Vec<(String, String)> = entries
            .iter()
            .filter(|(id, _)| !held(id))
            .filter_map(|(id, record)| Some((record["ended_at"].as_str()?.to_string(), id.clone())))
            .collect();
        ended.sort();
        let excess = ended.len().saturating_sub(keep);
        let forgotten: Vec<String> = ended
            .into_iter()
            .enumerate()
            .filter(|(n, (ended_at, _))| *n < excess || ended_at.as_str() < cutoff)
            .map(|(_, (_, id))| id)
            .collect();
        for id in &forgotten {
            entries.remove(id);
        }
        if !forgotten.is_empty() {
            self.changed.notify_one();
        }
        forgotten
    }

    /// Whether a terminal has a record.
    pub fn has(&self, id: &str) -> bool {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(id)
    }

    pub fn remove(&self, id: &str) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries.remove(id).is_some() {
            self.changed.notify_one();
        }
    }

    /// Writes the file after each change, at most once a second. Runs
    /// until the host stops.
    pub async fn writer(&self) {
        loop {
            self.changed.notified().await;
            let _ = self.write();
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Writes every record to a temporary file, flushes it and renames it
    /// into place. The records are read under the same lock, so a write
    /// never replaces a newer one with an older copy. The host calls it as
    /// it stops, since the writer may not have written the last changes.
    pub fn write(&self) -> io::Result<()> {
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let temporary = self.path.with_extension("json.tmp");
        let mut file = crate::private(OpenOptions::new().write(true).create(true).truncate(true))
            .open(&temporary)?;
        let list: Vec<&Value> = entries.values().collect();
        file.write_all(serde_json::to_string(&list)?.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn records_written_at_once_from_two_threads_all_land() {
        let dir = tempfile::tempdir().unwrap();
        let records = Arc::new(Records::new(dir.path().join("terminals.json")));
        let writers: Vec<_> = (0..2)
            .map(|n| {
                let records = records.clone();
                std::thread::spawn(move || {
                    for i in 0..50 {
                        let id = format!("t_{n}_{i}");
                        records.insert_now(&id, json!({"id": id})).unwrap();
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let written: Vec<Value> =
            serde_json::from_str(&fs::read_to_string(dir.path().join("terminals.json")).unwrap())
                .unwrap();
        assert_eq!(written.len(), 100);
    }
}
