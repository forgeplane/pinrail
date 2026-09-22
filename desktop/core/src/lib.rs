//! wicket-core is the review server as a library. The desktop app embeds it,
//! headless mode runs it without a window, and the CLI talks to it over HTTP.
//! [`Wicket::open`] initializes the application without starting HTTP; the
//! desktop and [`api`] share its review service and settings operations.
//!
//! A requester submits a review using a plugin; the editor returns a
//! decision; a revised submission is a new round of the same review. Nothing
//! is edited after it is written: status is derived from what exists.

pub mod api;
pub mod app;
pub mod config;
pub mod db;
#[cfg(feature = "docs")]
pub mod docs;
pub mod error;
pub mod events;
pub mod id;
pub mod markdown;
pub mod plugins;
pub mod reviews;
pub mod schema;
pub mod server_info;
pub mod settings;

pub use app::Wicket;
pub use config::Config;
pub use error::{Error, Violation};

/// The version of this crate, reported by `/api/v1/info`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
