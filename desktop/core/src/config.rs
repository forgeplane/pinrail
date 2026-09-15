//! Where the server keeps its data, who the editor is, and which port it
//! listens on.

use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

/// The port the Elixir server has always used; the CLI falls back to it.
pub const DEFAULT_PORT: u16 = 4747;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub data_dir: PathBuf,
    pub port: u16,
    /// Recorded as `decided_by` on every decision made through this server.
    pub user: String,
    /// The directory served at `/sdk/v1/`; none means the SDK is not served.
    pub sdk_dir: Option<PathBuf>,
    /// Extra plugin directories, on top of the built-in and user ones.
    pub plugin_dirs: Vec<PathBuf>,
}

impl Config {
    /// Reads `WICKET_DATA_DIR`, `WICKET_PORT`, `WICKET_USER` and
    /// `WICKET_SDK_DIR` from the environment, with the same data dir fallbacks
    /// as the Elixir server: `$XDG_DATA_HOME/wicket`, then
    /// `~/.local/share/wicket`, on every platform.
    pub fn from_env() -> Self {
        let mut config = Self::from_vars(
            env::var_os("WICKET_DATA_DIR").map(PathBuf::from),
            env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            env::var_os("HOME").map(PathBuf::from),
            env::var("WICKET_PORT").ok(),
        );
        // the file's port, unless the environment says otherwise
        if env::var_os("WICKET_PORT").is_none()
            && let Some(port) = crate::settings::port_in(&config.data_dir)
        {
            config.port = port;
        }
        config.user = env::var("WICKET_USER")
            .or_else(|_| env::var("USER"))
            .ok()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| "wicket".to_string());
        config.sdk_dir = env::var_os("WICKET_SDK_DIR").map(PathBuf::from);
        config
    }

    pub fn new(data_dir: impl Into<PathBuf>, port: u16) -> Self {
        Self {
            data_dir: data_dir.into(),
            port,
            user: "wicket".to_string(),
            sdk_dir: None,
            plugin_dirs: Vec::new(),
        }
    }

    fn from_vars(
        data_dir: Option<PathBuf>,
        xdg_data_home: Option<PathBuf>,
        home: Option<PathBuf>,
        port: Option<String>,
    ) -> Self {
        let data_dir = data_dir
            .or_else(|| xdg_data_home.map(|d| d.join("wicket")))
            .or_else(|| home.map(|h| h.join(".local/share/wicket")))
            .unwrap_or_else(|| PathBuf::from("wicket-data"));
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
        self.data_dir.join("wicket.db")
    }

    /// Where the plugin embedded in the binary is written out.
    pub fn builtin_plugins_dir(&self) -> PathBuf {
        self.data_dir.join("builtin")
    }

    /// Where a person drops their own plugins.
    pub fn user_plugins_dir(&self) -> PathBuf {
        self.data_dir.join("user-plugins")
    }

    /// Plugin version snapshots live under here.
    pub fn snapshots_dir(&self) -> PathBuf {
        self.data_dir.join("plugins")
    }

    /// Installed plugins: one entry per plugin and major version.
    pub fn plugin_store_dir(&self) -> PathBuf {
        self.data_dir.join("plugins").join("store")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(c.data_dir, PathBuf::from("/xdg/wicket"));
        let c = Config::from_vars(None, None, Some("/home/u".into()), None);
        assert_eq!(c.data_dir, PathBuf::from("/home/u/.local/share/wicket"));
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
