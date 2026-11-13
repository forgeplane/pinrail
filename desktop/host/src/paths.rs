//! Where the host keeps its files, and where its socket lives.

use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The longest socket path the system accepts, without the final NUL.
const MAX_SOCKET_PATH: usize = if cfg!(target_os = "macos") { 103 } else { 107 };

/// The host's files for one data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `<data dir>/host`
    pub host_dir: PathBuf,
    /// The folder of the host's socket and the holders' sockets.
    pub socket_dir: PathBuf,
}

impl Paths {
    /// The paths for `data_dir`, with the runtime directory the environment
    /// names: `$XDG_RUNTIME_DIR` on Linux, `$TMPDIR` on macOS, else
    /// `/tmp/pinrail-<uid>`.
    pub fn new(data_dir: &Path) -> Self {
        let runtime = runtime_dir();
        Self::with_runtime_dir(data_dir, runtime.as_deref(), crate::sys::uid())
    }

    /// The paths with the runtime directory given. Each data directory has
    /// its own socket folder, named after a hash of its path, so that two
    /// apps with different data directories, such as tests, do not meet.
    /// When the socket's path would be too long for the system, the folder
    /// moves to `/tmp/pinrail-<uid>/`.
    pub fn with_runtime_dir(data_dir: &Path, runtime: Option<&Path>, uid: u32) -> Self {
        let fallback = PathBuf::from(format!("/tmp/pinrail-{uid}"));
        let hash = short_hash(data_dir);
        let preferred = runtime
            .unwrap_or(&fallback)
            .join(format!("pinrail-host-{hash}"));
        let socket_dir = if fits(&preferred) {
            preferred
        } else {
            fallback.join(&hash)
        };
        Self {
            host_dir: data_dir.join("host"),
            socket_dir,
        }
    }

    pub fn socket(&self) -> PathBuf {
        self.socket_dir.join("host.sock")
    }

    pub fn registration(&self) -> PathBuf {
        self.host_dir.join("host.json")
    }

    pub fn lock(&self) -> PathBuf {
        self.host_dir.join("host.lock")
    }

    pub fn log(&self) -> PathBuf {
        self.host_dir.join("host.log")
    }

    /// Creates the host's folder and the socket's folder, readable by
    /// their owner only. A socket folder that exists must belong to this
    /// user and be closed to others; it is never changed with `chmod`, so
    /// a folder someone else prepared is refused rather than taken over.
    pub fn create(&self) -> io::Result<()> {
        private_dir(&self.host_dir)?;
        // the shared fallback folder, which another user could have made
        let fallback = PathBuf::from(format!("/tmp/pinrail-{}", crate::sys::uid()));
        if self.socket_dir.parent() == Some(fallback.as_path()) {
            owned_dir(&fallback)?;
        }
        owned_dir(&self.socket_dir)
    }
}

/// The data directory the core uses when it is not told one:
/// `$PINRAIL_DATA_DIR`, else `$XDG_DATA_HOME/pinrail`, else
/// `~/.local/share/pinrail`.
pub fn default_data_dir() -> PathBuf {
    let var = |name| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    var("PINRAIL_DATA_DIR")
        .or_else(|| var("XDG_DATA_HOME").map(|d| d.join("pinrail")))
        .or_else(|| var("HOME").map(|h| h.join(".local/share/pinrail")))
        .unwrap_or_else(|| PathBuf::from("pinrail-data"))
}

fn runtime_dir() -> Option<PathBuf> {
    let var = if cfg!(target_os = "macos") {
        "TMPDIR"
    } else {
        "XDG_RUNTIME_DIR"
    };
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The first 12 hex digits of the SHA-256 of the path.
fn short_hash(path: &Path) -> String {
    let digest = Sha256::digest(path.as_os_str().as_encoded_bytes());
    digest[..6].iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether the longest socket in `dir` fits the system's limit. The
/// holders' sockets, `holder.<8 hex digits>.sock`, are the longest.
fn fits(dir: &Path) -> bool {
    dir.join("holder.00000000.sock").as_os_str().len() <= MAX_SOCKET_PATH
}

/// Creates `dir` with mode 0700, or closes an existing one of ours.
fn private_dir(dir: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    if fs::metadata(dir)?.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Creates `dir` with mode 0700, or checks that the one there belongs to
/// this user and is closed to others.
fn owned_dir(dir: &Path) -> io::Result<()> {
    match DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = fs::symlink_metadata(dir)?;
    if !metadata.is_dir()
        || metadata.uid() != crate::sys::uid()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} must be a folder of this user that others cannot open",
                dir.display()
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_socket_folder_is_in_the_runtime_directory_and_named_for_the_data_directory() {
        let paths = Paths::with_runtime_dir(
            Path::new("/Users/p/.local/share/pinrail"),
            Some(Path::new("/run/user/501")),
            501,
        );
        let name = paths.socket_dir.file_name().unwrap().to_str().unwrap();
        assert!(paths.socket_dir.starts_with("/run/user/501"));
        assert!(
            name.starts_with("pinrail-host-") && name.len() == 25,
            "{name}"
        );
        assert_eq!(paths.socket(), paths.socket_dir.join("host.sock"));
        assert_eq!(
            paths.registration(),
            Path::new("/Users/p/.local/share/pinrail/host/host.json")
        );
        // another data directory gets another folder
        let other = Paths::with_runtime_dir(
            Path::new("/tmp/test-data"),
            Some(Path::new("/run/user/501")),
            501,
        );
        assert_ne!(other.socket_dir, paths.socket_dir);
    }

    #[test]
    fn a_socket_path_too_long_moves_to_a_short_folder() {
        let long = PathBuf::from("/var/folders/").join("x".repeat(90));
        let paths = Paths::with_runtime_dir(Path::new("/data"), Some(&long), 501);
        assert!(paths.socket_dir.starts_with("/tmp/pinrail-501"));
        assert!(fits(&paths.socket_dir));
    }

    #[test]
    fn without_a_runtime_directory_the_socket_goes_under_tmp() {
        let paths = Paths::with_runtime_dir(Path::new("/data"), None, 501);
        assert!(paths.socket_dir.starts_with("/tmp/pinrail-501/"));
    }

    #[test]
    fn a_socket_folder_open_to_others_is_refused_and_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("shared");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(owned_dir(&dir).is_err());
        let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o777);
    }
}
