//! Plugin definitions, registry and application operations.

mod install;
mod jobs;
mod manifest;
mod registry;
mod service;

pub use install::Options as InstallOptions;
pub(crate) use install::tidy;
pub use jobs::Job as InstallJob;
pub(crate) use manifest::valid_name;
pub use manifest::{Install, Plugin, version_of};
pub use registry::{Registry, hash_dir, hash_dir_where, install_builtin, plugin_subdirs};
pub use service::{PluginService, UpdateOutcome};
