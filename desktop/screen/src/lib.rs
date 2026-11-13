//! The terminal screen of an agent's run.
//!
//! The `ghostty` feature links libghostty-vt, which `build.rs` builds from
//! the release `ghostty.lock` pins.

// the terminal host runs agents in PTYs, so only on Unix
#[cfg(all(feature = "ghostty", unix))]
pub mod ghostty;
