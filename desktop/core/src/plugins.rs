//! Plugin definitions, registry and application operations.

mod registry;
mod service;

pub(crate) use registry::valid_name;
pub use registry::{
    Install, Plugin, Registry, hash_dir, hash_dir_where, install_builtin, plugin_subdirs,
    version_of,
};
pub use service::PluginService;
