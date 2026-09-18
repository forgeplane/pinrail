//! Plugin definitions, registry and application operations.
//!
//! [`PluginService`] is the way in: listing, installing, updating and removing
//! are application operations, and the registry behind them is the crate's own
//! business. What stays public is the vocabulary a caller reads from a review
//! or a manifest.

mod install;
mod jobs;
mod manifest;
mod registry;
mod service;

pub use install::Options as InstallOptions;
pub use jobs::Job as InstallJob;
pub use manifest::{Install, Plugin};
pub use service::{PluginService, UpdateOutcome};

pub(crate) use install::tidy;
pub(crate) use manifest::version_of;
pub(crate) use registry::{Registry, install_builtin, plugin_subdirs};
