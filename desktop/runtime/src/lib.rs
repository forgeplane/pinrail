//! What Pinrail knows about each coding agent, shared by the app and the
//! CLI: how to tell which agent a command runs under, and where each agent
//! keeps its configuration and skills.

pub mod detect;
pub mod registry;
pub mod skills;

pub use registry::{AgentSpec, Folders, all, find};
