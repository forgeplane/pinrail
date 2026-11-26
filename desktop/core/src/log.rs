//! The core's log, through `tracing`: each event goes to stderr and to
//! `core.log` in the data folder. The file is rotated at 5 MiB, and 3
//! older files are kept.
//!
//! The level is `info` unless `PINRAIL_LOG` sets another, in the syntax of
//! `RUST_LOG`, such as `debug` or `pinrail_core::api=debug`.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const ROTATE_AT: u64 = 5 * 1024 * 1024;
const KEPT: usize = 3;
/// The variable that sets the level, as `RUST_LOG` does.
pub const LEVEL_VAR: &str = "PINRAIL_LOG";

struct LogFile {
    path: PathBuf,
    file: Option<File>,
}

static FILE: Mutex<Option<LogFile>> = Mutex::new(None);

/// Logs to stderr and to the file at `path` from now on. Only the first
/// call in a process installs the logger; a later one moves the file.
pub fn init(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    *FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(LogFile {
        path: path.to_path_buf(),
        file: open_file(path),
    });
    let filter = EnvFilter::try_from_env(LEVEL_VAR).unwrap_or_else(|_| EnvFilter::new("info"));
    let stderr = tracing_subscriber::fmt::layer().with_writer(io::stderr);
    let file = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(|| FileWriter);
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(stderr)
        .with(file)
        .try_init();
}

/// Writes to the log file, rotating it first when it is full. A log file
/// that cannot be written is skipped.
struct FileWriter;

impl Write for FileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut log = FILE.lock().unwrap_or_else(|e| e.into_inner());
        let Some(log) = log.as_mut() else {
            return Ok(buf.len());
        };
        if log
            .file
            .as_ref()
            .and_then(|f| f.metadata().ok())
            .is_some_and(|m| m.len() >= ROTATE_AT)
        {
            rotate(&log.path);
            log.file = open_file(&log.path);
        }
        if let Some(file) = log.file.as_mut() {
            let _ = file.write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn open_file(path: &Path) -> Option<File> {
    let mut options = OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path).ok()
}

/// `core.log` becomes `core.log.1`, which becomes `core.log.2`, and so on.
/// The oldest is dropped.
fn rotate(path: &Path) {
    let numbered = |n: usize| PathBuf::from(format!("{}.{n}", path.display()));
    for n in (1..KEPT).rev() {
        let _ = fs::rename(numbered(n), numbered(n + 1));
    }
    let _ = fs::rename(path, numbered(1));
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_full_log_is_rotated_and_three_older_files_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.log");
        for n in 1..=4 {
            std::fs::write(&path, format!("log {n}")).unwrap();
            super::rotate(&path);
        }
        let read = |name: &str| std::fs::read_to_string(dir.path().join(name)).ok();
        assert_eq!(read("core.log.1").as_deref(), Some("log 4"));
        assert_eq!(read("core.log.3").as_deref(), Some("log 2"));
        assert_eq!(read("core.log.4"), None);
        assert_eq!(read("core.log"), None);
    }
}
