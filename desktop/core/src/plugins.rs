//! Plugin definitions, registry and application operations.
//!
//! [`PluginService`] is the way in: listing, installing and removing
//! are application operations, and the registry behind them is the crate's own
//! business. What stays public is the vocabulary a caller reads from a review
//! or a manifest.

pub(crate) mod bundles;
mod catalog;
pub(crate) mod install;
mod registry;
mod service;

use pinrail_format::{manifest, sample, summary};

pub use bundles::Bundles;
pub use catalog::Catalog;
pub use install::Options as InstallOptions;
/// The manifest's JSON Schema, for the docs' manifest reference.
#[cfg(feature = "docs")]
pub use manifest::{FEATURES as MANIFEST_FEATURES, SCHEMA as MANIFEST_SCHEMA};
pub use manifest::{Install, Plugin};
pub use sample::Sample;
pub use service::PluginService;
pub use summary::Declaration as SummaryDeclaration;

#[cfg(test)]
pub(crate) use catalog::carried;
pub(crate) use install::tidy;
pub(crate) use manifest::version_of;
pub(crate) use registry::Registry;
