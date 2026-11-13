//! `host.log`: a line per event, rotated at 5 MiB with 3 older files kept.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const ROTATE_AT: u64 = 5 * 1024 * 1024;
const KEPT: usize = 3;

pub struct Log {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl Log {
    pub fn open(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            file: Mutex::new(open(path)),
        }
    }

    /// Writes a line with the time. A log that cannot be written is
    /// skipped: the host keeps running without it.
    pub fn line(&self, text: &str) {
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if file
            .as_ref()
            .and_then(|f| f.metadata().ok())
            .is_some_and(|m| m.len() >= ROTATE_AT)
        {
            rotate(&self.path);
            *file = open(&self.path);
        }
        if let Some(file) = file.as_mut() {
            let _ = writeln!(file, "{} {text}", crate::now());
        }
    }
}

fn open(path: &Path) -> Option<File> {
    OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(path)
        .ok()
}

/// `host.log` becomes `host.log.1`, which becomes `host.log.2`, and so on;
/// the oldest is dropped.
fn rotate(path: &Path) {
    let numbered = |n: usize| PathBuf::from(format!("{}.{n}", path.display()));
    for n in (1..KEPT).rev() {
        let _ = fs::rename(numbered(n), numbered(n + 1));
    }
    let _ = fs::rename(path, numbered(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_rotates_and_keeps_three_older_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("host.log");
        let log = Log::open(&path);
        let line = "x".repeat(1024 * 1024);
        for _ in 0..30 {
            log.line(&line);
        }
        let mut names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["host.log", "host.log.1", "host.log.2", "host.log.3"]
        );
        assert!(fs::metadata(&path).unwrap().len() <= ROTATE_AT + 2 * 1024 * 1024);
    }
}
