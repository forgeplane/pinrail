//! wicket-core is the review server as a library. The desktop app embeds it,
//! headless mode runs it without a window, and the CLI talks to it over HTTP.

pub mod api;
pub mod config;

pub use config::Config;

/// The version of this crate, reported by `/api/v1/info`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
