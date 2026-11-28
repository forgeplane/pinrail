//! What starting a program in a terminal means on every system: the
//! program started, why it could not start, and how it ended. `pty`
//! re-exports these with the system's own terminal.

use std::io;

/// A started program, with the host's side of its terminal.
pub struct Spawned {
    pub pid: u32,
    /// Its start time, as `pinrail-runtime` reads it for every comparison.
    pub started: Option<String>,
    pub master: crate::pty::Master,
}

impl std::fmt::Debug for Spawned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Spawned")
            .field("pid", &self.pid)
            .field("started", &self.started)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum SpawnError {
    /// The folder does not exist or is not a folder.
    FolderMissing,
    /// The program is not on the environment's `PATH`.
    NotFound(String),
    Io(io::Error),
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnError::FolderMissing => write!(f, "the folder does not exist"),
            SpawnError::NotFound(name) => write!(f, "{name} was not found"),
            SpawnError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SpawnError {}

impl From<io::Error> for SpawnError {
    fn from(error: io::Error) -> Self {
        SpawnError::Io(error)
    }
}

/// How a program ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    Signal(i32),
}

/// Sends a request to the holder that keeps a terminal, without waiting
/// for its answer: what the host asks of a terminal it cannot act on
/// itself, as on Windows, where only the holder can resize or close a
/// pseudo console.
pub type Control = Box<dyn Fn(serde_json::Value) + Send + Sync>;
