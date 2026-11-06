//! Work items: tasks the person and agents record, break down and work
//! through, each in a repository that numbers it, with sub-items and the
//! items it depends on.

pub mod keys;
mod model;

pub use keys::{Key, derive_prefix, valid_prefix};
pub use model::{Actor, Breakdown, BreakdownItem, ItemRef, NewItem, Status};
