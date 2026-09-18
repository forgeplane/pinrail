//! Plugin definitions, registry and application operations.

mod install;
mod jobs;
mod registry;
mod service;

pub use install::Options as InstallOptions;
pub(crate) use install::tidy;
pub use jobs::Job as InstallJob;
pub(crate) use registry::valid_name;
pub use registry::{
    Install, Plugin, Registry, hash_dir, hash_dir_where, install_builtin, plugin_subdirs,
    version_of,
};
pub use service::{PluginService, UpdateOutcome};
