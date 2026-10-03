//! Where the server keeps its data, who the editor is, and which port it
//! listens on.

use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

/// The port the server listens on unless told otherwise; the CLI falls back to it.
pub const DEFAULT_PORT: u16 = 4747;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub data_dir: PathBuf,
    pub port: u16,
    /// Recorded as `decided_by` on every decision made through this server.
    pub user: String,
    /// The directory served at `/sdk/v1/`; none means the SDK is not served.
    pub sdk_dir: Option<PathBuf>,
    /// The most one uploaded attachment may be, in bytes.
    pub max_attachment_bytes: u64,
}

/// 100 MB: a model, a recording, a document with its images.
pub use pinrail_format::attachments::MAX_ATTACHMENT_BYTES;

impl Config {
    /// Reads `PINRAIL_DATA_DIR`, `PINRAIL_PORT`, `PINRAIL_USER` and
    /// `PINRAIL_SDK_DIR` from the environment. The data dir falls back to
    /// `$XDG_DATA_HOME/pinrail`, then `~/.local/share/pinrail`, on every
    /// platform, where the CLI looks for it too.
    pub fn from_env() -> Self {
        let mut config = Self::from_vars(
            env::var_os("PINRAIL_DATA_DIR").map(PathBuf::from),
            env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            env::var_os("HOME").map(PathBuf::from),
            env::var("PINRAIL_PORT").ok(),
        );
        config.port = resolve_port(
            env::var("PINRAIL_PORT").ok().as_deref(),
            crate::settings::port_in(&config.data_dir),
        );
        config.user = env::var("PINRAIL_USER")
            .or_else(|_| env::var("USER"))
            .ok()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| "pinrail".to_string());
        config.sdk_dir = env::var_os("PINRAIL_SDK_DIR").map(PathBuf::from);
        config
    }

    pub fn new(data_dir: impl Into<PathBuf>, port: u16) -> Self {
        Self {
            data_dir: data_dir.into(),
            port,
            user: "pinrail".to_string(),
            sdk_dir: None,
            max_attachment_bytes: MAX_ATTACHMENT_BYTES,
        }
    }

    fn from_vars(
        data_dir: Option<PathBuf>,
        xdg_data_home: Option<PathBuf>,
        home: Option<PathBuf>,
        port: Option<String>,
    ) -> Self {
        let data_dir = data_dir
            .or_else(|| xdg_data_home.map(|d| d.join("pinrail")))
            .or_else(|| home.map(|h| h.join(".local/share/pinrail")))
            .unwrap_or_else(|| PathBuf::from("pinrail-data"));
        let port = port.and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
        Self::new(data_dir, port)
    }

    /// The server only ever listens on the loopback address.
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.port)
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.bind_addr())
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("pinrail.db")
    }

    /// Attachments: `sha256/<first two>/<hash>`, and `tmp/` for uploads in flight.
    pub fn attachments_dir(&self) -> PathBuf {
        self.data_dir.join("attachments")
    }

    /// Plugin bundles: one folder per bundle, named by its hash.
    pub fn plugin_bundles_dir(&self) -> PathBuf {
        self.plugins_dir().join("bundles")
    }

    /// Plugins: `bundles/`, and `work/` and `logs/` for fetches and builds.
    pub fn plugins_dir(&self) -> PathBuf {
        self.data_dir.join("plugins")
    }
}

/// The port to serve on: `PINRAIL_PORT` when it is a port, else the one
/// settings.json names, else the default.
fn resolve_port(env: Option<&str>, file: Option<u16>) -> u16 {
    env.and_then(|p| p.trim().parse().ok())
        .filter(|p| *p != 0)
        .or(file)
        .unwrap_or(DEFAULT_PORT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_port_wins_over_the_file_and_the_file_over_the_default() {
        assert_eq!(resolve_port(Some("4800"), Some(4900)), 4800);
        assert_eq!(resolve_port(None, Some(4900)), 4900);
        assert_eq!(resolve_port(None, None), DEFAULT_PORT);
        // a value that is not a port counts as not set
        assert_eq!(resolve_port(Some("nope"), Some(4900)), 4900);
        assert_eq!(resolve_port(Some("0"), None), DEFAULT_PORT);
    }

    #[test]
    fn explicit_data_dir_wins() {
        let c = Config::from_vars(
            Some("/tmp/w".into()),
            Some("/xdg".into()),
            Some("/home/u".into()),
            None,
        );
        assert_eq!(c.data_dir, PathBuf::from("/tmp/w"));
        assert_eq!(c.port, DEFAULT_PORT);
    }

    #[test]
    fn xdg_then_home() {
        let c = Config::from_vars(None, Some("/xdg".into()), Some("/home/u".into()), None);
        assert_eq!(c.data_dir, PathBuf::from("/xdg/pinrail"));
        let c = Config::from_vars(None, None, Some("/home/u".into()), None);
        assert_eq!(c.data_dir, PathBuf::from("/home/u/.local/share/pinrail"));
    }

    #[test]
    fn port_parses_or_defaults() {
        let c = Config::from_vars(Some("/d".into()), None, None, Some("5000".into()));
        assert_eq!(c.port, 5000);
        let c = Config::from_vars(Some("/d".into()), None, None, Some("nope".into()));
        assert_eq!(c.port, DEFAULT_PORT);
        assert_eq!(c.bind_addr().to_string(), "127.0.0.1:4747");
        assert_eq!(c.url(), "http://127.0.0.1:4747");
    }
}
