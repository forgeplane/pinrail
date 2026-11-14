//! `host.json`: how the core finds the running host and proves itself to
//! it.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// What a running host writes about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub pid: u32,
    pub started_at: String,
    pub boot_id: String,
    pub version: String,
    /// The protocol generation.
    pub protocol: u32,
    pub features: Vec<String>,
    pub socket: PathBuf,
    /// 32 random bytes, hex-encoded, which a controller's `hello` presents.
    pub secret: String,
    /// The build it runs, `<version>-<sha8>` of its binary, which a core
    /// compares with its own host's.
    pub build: Option<String>,
}

impl Registration {
    pub fn to_json(&self) -> Value {
        json!({
            "pid": self.pid,
            "started_at": self.started_at,
            "boot_id": self.boot_id,
            "version": self.version,
            "protocol": self.protocol,
            "features": self.features,
            "socket": self.socket,
            "secret": self.secret,
            "build": self.build,
        })
    }

    pub fn from_json(value: &Value) -> Option<Self> {
        let text = |key: &str| value.get(key)?.as_str().map(str::to_string);
        Some(Self {
            pid: u32::try_from(value.get("pid")?.as_u64()?).ok()?,
            started_at: text("started_at")?,
            boot_id: text("boot_id")?,
            version: text("version")?,
            protocol: u32::try_from(value.get("protocol")?.as_u64()?).ok()?,
            features: value
                .get("features")?
                .as_array()?
                .iter()
                .filter_map(|f| f.as_str().map(str::to_string))
                .collect(),
            socket: PathBuf::from(text("socket")?),
            secret: text("secret")?,
            build: text("build"),
        })
    }

    /// The registration at `path`, if there is one that reads.
    pub fn read(path: &Path) -> Option<Self> {
        let text = fs::read_to_string(path).ok()?;
        Self::from_json(&serde_json::from_str(&text).ok()?)
    }

    /// Writes the registration readable by its owner only, to a temporary
    /// file renamed into place, so that a reader never sees half of it.
    pub fn write(&self, path: &Path) -> io::Result<()> {
        let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(self.to_json().to_string().as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    }

    /// Whether the host it names may still run: its process exists and it
    /// started in this boot. A host from another boot is never adopted.
    /// The socket's `hello` still has to confirm it, as the process id may
    /// have been reused.
    pub fn may_be_live(&self) -> bool {
        Some(&self.boot_id) == crate::boot_id().as_ref() && crate::sys::alive(self.pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration(boot_id: String, pid: u32) -> Registration {
        Registration {
            pid,
            started_at: crate::now(),
            boot_id,
            version: "0.6.0".to_string(),
            protocol: 1,
            features: vec![],
            socket: PathBuf::from("/tmp/host.sock"),
            secret: crate::random_hex(32),
            build: None,
        }
    }

    #[test]
    fn a_registration_is_written_privately_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("host.json");
        let written = registration("boot".to_string(), 42);
        written.write(&path).unwrap();
        assert_eq!(Registration::read(&path), Some(written));
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        // nothing else is left in the folder
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_live_process_of_this_boot_may_be_the_host() {
        let this = registration(crate::boot_id().unwrap(), std::process::id());
        assert!(this.may_be_live());
    }

    #[test]
    fn a_host_from_another_boot_is_never_live() {
        let other = registration("another-boot".to_string(), std::process::id());
        assert!(!other.may_be_live());
    }
}
