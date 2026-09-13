//! Where the server keeps its data and which port it listens on.

use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

/// The port the Elixir server has always used; the CLI falls back to it.
pub const DEFAULT_PORT: u16 = 4747;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub data_dir: PathBuf,
    pub port: u16,
}

impl Config {
    /// Reads `WICKET_DATA_DIR` and `WICKET_PORT` from the environment, with the
    /// same fallbacks as the Elixir server: `$XDG_DATA_HOME/wicket`, then
    /// `~/.local/share/wicket`, on every platform.
    pub fn from_env() -> Self {
        Self::from_vars(
            env::var_os("WICKET_DATA_DIR").map(PathBuf::from),
            env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            env::var_os("HOME").map(PathBuf::from),
            env::var("WICKET_PORT").ok(),
        )
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
        Self { data_dir, port }
    }

    /// The server only ever listens on the loopback address.
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.port)
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
    }
}
