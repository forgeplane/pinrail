//! Application settings: a public service backed by a private file store.
//! [`SettingsService`] handles reads, validated changes and notifications;
//! [`port_in`] reads the configured port before the application is opened.

mod service;
mod store;

pub use service::SettingsService;
pub use store::port_in;
#[cfg(feature = "docs")]
pub use store::reference;

/// The group that holds each plugin's own settings, plugin name to an
/// object of the values someone changed; the shape of the values is the
/// plugin's schema, checked by the settings service.
pub const PLUGINS: &str = "/plugins";
