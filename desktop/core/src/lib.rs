//! wicket-core is the review server as a library. The desktop app embeds it,
//! headless mode runs it without a window, and the CLI talks to it over HTTP.
//!
//! A requester submits a review using a plugin; the editor returns a
//! decision; a revised submission is a new round of the same review. Nothing
//! is edited after it is written: status is derived from what exists.

pub mod api;
pub mod app;
pub mod config;
pub mod db;
pub mod error;
pub mod events;
pub mod id;
pub mod install;
pub mod markdown;
pub mod plugins;
pub mod review;
pub mod reviews;
pub mod schema;
pub mod server_info;
pub mod settings;

pub use app::Wicket;
pub use config::Config;
pub use error::{Error, Violation};

/// The version of this crate, reported by `/api/v1/info`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
