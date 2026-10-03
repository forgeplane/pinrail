//! Application settings: a public service backed by a private file store.
//! [`SettingsService`] handles reads, validated changes and notifications;
//! [`port_in`] reads the configured port before the application is opened.

mod service;
mod store;

pub use service::SettingsService;
pub use store::port_in;
#[cfg(feature = "docs")]
pub use store::reference;

/// The group that holds each plugin's own settings, its name to an object
/// of the values someone changed; the shape of the values is the
/// plugin's schema, checked by the settings service.
pub const PLUGINS: &str = "/plugins";
/// Where each plugin's permission to open links lives: the origins it may
/// open without asking, and the source it was installed from when they
/// were allowed. The app writes it; a plugin's view cannot.
pub const LINKS: &str = "/links";

/// A key as one part of a JSON pointer, its `~` and `/` escaped. A
/// plugin's name holds neither, but a key someone wrote by hand might.
pub fn pointer_part(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}
