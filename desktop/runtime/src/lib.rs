//! What Pinrail knows about each coding agent, shared by the app and the
//! CLI: how to tell which agent a command runs under, and where each agent
//! keeps its configuration and skills.

pub mod agent;
mod agents;
pub mod detect;
pub mod hooks;
pub mod integration;
pub mod process;
pub mod registry;
pub mod skills;

pub use agent::{Agent, Env, Hint, HintStatus, adapter};
pub use registry::{AgentSpec, Folders, all, find};
